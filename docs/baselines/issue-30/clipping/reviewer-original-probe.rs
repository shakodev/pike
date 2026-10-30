// Reviewer-owned rendered-pixel oracle: changing a title may not paint in empty chrome.
use iced::advanced::renderer::Headless;
use iced::advanced::widget::Tree;
use iced::advanced::{Layout, Renderer as _, layout, mouse, renderer};
use iced::{Executor, Rectangle, Size};
use pike::executor::SharedExecutor;
use pike::identity::TabId;
use pike::tab_strip::{self, Item, TabAction};
use pike::tabs::{Activity, Title};
use pike::workspace::Model;

#[derive(Clone)]
enum Message { Activate, Close, Action }
fn activate(_: TabId) -> Message { Message::Activate }
fn close(_: TabId) -> Message { Message::Close }
fn action(_: TabAction) -> Message { Message::Action }

fn render(renderer: &mut iced::Renderer, id: TabId, title: &str) -> Vec<u8> {
    let size = Size::new(1100.0, 100.0);
    let viewport = Rectangle::with_size(size);
    let mut view = tab_strip::view(vec![Item { id, title: Title::new(title).unwrap(), activity: Activity::default() }], Some(id), activate, close, action);
    let mut tree = Tree::new(view.as_widget());
    let node = view.as_widget_mut().layout(&mut tree, renderer, &layout::Limits::new(Size::ZERO, size));
    renderer.reset(viewport);
    view.as_widget().draw(&tree, renderer, &iced::Theme::Dark, &renderer::Style { text_color: iced::Color::WHITE }, Layout::new(&node), mouse::Cursor::Unavailable, &viewport);
    <iced::Renderer as Headless>::screenshot(renderer, Size::new(1100, 100), 1.0, iced::Color::BLACK)
}

fn main() {
    let executor = SharedExecutor::new().unwrap();
    let mut renderer = executor.block_on(<iced::Renderer as Headless>::new(iced::Font::MONOSPACE, iced::Pixels(14.0), None)).unwrap();
    let mut model = Model::default();
    let id = model.create_tab().unwrap().0.tab;
    let short = render(&mut renderer, id, "W");
    let long = render(&mut renderer, id, &"W".repeat(256));
    let root = std::path::Path::new("docs/baselines/issue-30-reviewer2");
    std::fs::write(root.join("clip-short.rgba"), &short).unwrap();
    std::fs::write(root.join("clip-long.rgba"), &long).unwrap();
    let mut changed = 0;
    for y in 0..35 {
        for x in 210..900 {
            let offset = (y * 1100 + x) * 4;
            changed += usize::from(short[offset..offset + 4] != long[offset..offset + 4]);
        }
    }
    println!("Changed pixels in empty chrome beyond the single tab and close control: {changed}");
    assert_eq!(changed, 0, "long title paints outside its tab into empty adjacent chrome");
}
