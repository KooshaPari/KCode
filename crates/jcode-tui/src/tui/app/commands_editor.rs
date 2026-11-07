pub(super) fn run_interactive_editor_with(
    command: &mut std::process::Command,
    suspend: impl FnOnce(),
    resume: impl FnOnce(),
) -> std::io::Result<std::process::ExitStatus> {
    suspend();
    let result = command.status();
    resume();
    result
}
