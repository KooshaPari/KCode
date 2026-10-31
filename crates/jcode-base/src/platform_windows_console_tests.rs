//! Native console ownership regression, isolated from the invoking terminal.
use std::fs::{File, OpenOptions};
use std::os::windows::io::AsRawHandle;
use std::process::{Command, Stdio};

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetConsoleMode(handle: *mut std::ffi::c_void, mode: *mut u32) -> i32;
    fn SetConsoleMode(handle: *mut std::ffi::c_void, mode: u32) -> i32;
}

fn console_handles() -> (File, File) {
    let open = |name| {
        OpenOptions::new()
            .read(true)
            .write(true)
            .open(name)
            .unwrap()
    };
    (open("CONIN$"), open("CONOUT$"))
}

fn mode(file: &File) -> u32 {
    let mut mode = 0;
    assert_ne!(
        unsafe { GetConsoleMode(file.as_raw_handle(), &mut mode) },
        0
    );
    mode
}

fn set_mode(file: &File, mode: u32) {
    assert_ne!(unsafe { SetConsoleMode(file.as_raw_handle(), mode) }, 0);
}

fn child(role: &str) -> Command {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            "platform::windows_console_tests::hidden_server_console_is_isolated",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("KCODE_CONSOLE_TEST_ROLE", role)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
}

#[test]
fn hidden_server_console_is_isolated() {
    match std::env::var("KCODE_CONSOLE_TEST_ROLE").as_deref() {
        Ok("child") => {
            let (input, output) = console_handles();
            // Valid independent changes: echo input and VT output processing.
            set_mode(&input, mode(&input) ^ 0x0004);
            set_mode(&output, mode(&output) ^ 0x0004);
        }
        Ok("parent") => {
            let (input, output) = console_handles();
            let initial = (mode(&input), mode(&output));
            let shared = child("child").output().unwrap();
            assert!(shared.status.success(), "{shared:?}");
            let changed = (mode(&input), mode(&output));
            // Negative control: redirected stdio alone still shares console modes.
            assert_eq!(changed, (initial.0 ^ 4, initial.1 ^ 4));
            set_mode(&input, initial.0);
            set_mode(&output, initial.1);

            let isolated = super::spawn_detached_without_console_window(&mut child("child"))
                .unwrap()
                .wait_with_output()
                .unwrap();
            assert!(isolated.status.success(), "{isolated:?}");
            let preserved = (mode(&input), mode(&output));
            assert_eq!(
                preserved, initial,
                "hidden child changed parent console modes"
            );
            println!(
                "console modes: initial={initial:?}, shared={changed:?}, isolated={preserved:?}"
            );
        }
        _ => {
            // The outer test never changes the operator's console.
            let result = super::spawn_detached_without_console_window(&mut child("parent"))
                .unwrap()
                .wait_with_output()
                .unwrap();
            assert!(result.status.success(), "{result:?}");
            print!("{}", String::from_utf8_lossy(&result.stdout));
        }
    }
}
