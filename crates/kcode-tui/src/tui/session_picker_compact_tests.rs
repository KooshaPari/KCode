use super::*;

#[test]
fn tiny_picker_frames_render_without_panicking() {
    for (width, height) in [(38, 14), (20, 5), (57, 1), (1, 1)] {
        let mut picker = SessionPicker::new_grouped(Vec::new(), Vec::new());
        let backend = ratatui::backend::TestBackend::new(width, height);
        let mut terminal = ratatui::Terminal::new(backend).expect("terminal");
        terminal
            .draw(|frame| picker.render(frame))
            .expect("compact render");
        assert_eq!(terminal.backend().buffer()[(0, 0)].symbol(), "N");
    }
}
