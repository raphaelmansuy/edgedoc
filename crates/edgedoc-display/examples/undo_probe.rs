//! What one keystroke costs, and how many undo steps typing a word takes.
use edgedoc_display::Editor;

fn main() {
    let mut editor = Editor::blank().expect("blank");
    let t0 = std::time::Instant::now();
    for ch in "Bonjour le monde ".chars() {
        editor.insert_text(&ch.to_string()).expect("type");
    }
    let typing = t0.elapsed();
    println!("typing 17 chars: {:?} ({:?} per char)", typing, typing / 17);
    println!("undo steps for 17 keystrokes: {}", editor.undo_depth());
}
