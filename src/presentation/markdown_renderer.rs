use bevy::prelude::*;
use pulldown_cmark::{Event, Parser, Tag, TagEnd};

#[derive(Clone)]
pub enum MdBlock {
    Heading { level: u8, text: String },
    Paragraph { text: String },
    BlockQuote { text: String },
    CodeBlock { lang: String, text: String },
    UnorderedList { items: Vec<String> },
    HorizontalRule,
}

fn collect_inline_plain(events: &[Event]) -> String {
    let mut out = String::new();
    for ev in events {
        match ev {
            Event::Text(t) => out.push_str(t),
            Event::Code(t) => out.push_str(t),
            Event::SoftBreak | Event::HardBreak => out.push('\n'),
            _ => {}
        }
    }
    out
}

pub fn parse_markdown(text: &str) -> Vec<MdBlock> {
    let mut blocks = Vec::new();
    let parser = Parser::new(text);
    let events: Vec<Event> = parser.collect();
    let mut i = 0;

    while i < events.len() {
        match &events[i] {
            Event::Start(Tag::Heading { level, .. }) => {
                i += 1;
                let level_u8 = match level {
                    pulldown_cmark::HeadingLevel::H1 => 1,
                    pulldown_cmark::HeadingLevel::H2 => 2,
                    pulldown_cmark::HeadingLevel::H3 => 3,
                    pulldown_cmark::HeadingLevel::H4 => 4,
                    pulldown_cmark::HeadingLevel::H5 => 5,
                    pulldown_cmark::HeadingLevel::H6 => 6,
                };
                let mut content = Vec::new();
                while i < events.len() && !matches!(&events[i], Event::End(TagEnd::Heading(..))) {
                    content.push(events[i].clone());
                    i += 1;
                }
                let text = collect_inline_plain(&content);
                if !text.is_empty() {
                    blocks.push(MdBlock::Heading {
                        level: level_u8,
                        text,
                    });
                }
                i += 1;
            }
            Event::Start(Tag::Paragraph) => {
                i += 1;
                let mut content = Vec::new();
                while i < events.len() && !matches!(&events[i], Event::End(TagEnd::Paragraph)) {
                    content.push(events[i].clone());
                    i += 1;
                }
                let text = collect_inline_plain(&content);
                if !text.is_empty() {
                    blocks.push(MdBlock::Paragraph { text });
                }
                i += 1;
            }
            Event::Start(Tag::BlockQuote(_)) => {
                i += 1;
                let mut content = Vec::new();
                let mut depth = 1;
                while i < events.len() && depth > 0 {
                    if matches!(&events[i], Event::Start(Tag::BlockQuote(_))) {
                        depth += 1;
                    } else if matches!(&events[i], Event::End(TagEnd::BlockQuote(..))) {
                        depth -= 1;
                    }
                    if depth > 0 {
                        content.push(events[i].clone());
                    }
                    i += 1;
                }
                let text = collect_inline_plain(&content);
                if !text.is_empty() {
                    blocks.push(MdBlock::BlockQuote { text });
                }
            }
            Event::Start(Tag::CodeBlock(kind)) => {
                let lang = match kind {
                    pulldown_cmark::CodeBlockKind::Fenced(l) => l.to_string(),
                    _ => String::new(),
                };
                i += 1;
                let mut content = Vec::new();
                while i < events.len() && !matches!(&events[i], Event::End(TagEnd::CodeBlock)) {
                    if let Event::Text(t) = &events[i] {
                        content.push(t.clone());
                    }
                    i += 1;
                }
                let text = content.join("");
                blocks.push(MdBlock::CodeBlock { lang, text });
                i += 1;
            }
            Event::Start(Tag::List(..)) => {
                let mut items = Vec::new();
                i += 1;
                while i < events.len() && !matches!(&events[i], Event::End(TagEnd::List(..))) {
                    if matches!(&events[i], Event::Start(Tag::Item)) {
                        i += 1;
                        let mut content = Vec::new();
                        let mut depth = 1;
                        while i < events.len() && depth > 0 {
                            if matches!(&events[i], Event::Start(Tag::Item)) {
                                depth += 1;
                            } else if matches!(&events[i], Event::End(TagEnd::Item)) {
                                depth -= 1;
                            }
                            if depth > 0 {
                                content.push(events[i].clone());
                            }
                            i += 1;
                        }
                        let text = collect_inline_plain(&content);
                        if !text.is_empty() {
                            items.push(text);
                        }
                    } else {
                        i += 1;
                    }
                }
                if !items.is_empty() {
                    blocks.push(MdBlock::UnorderedList { items });
                }
                i += 1;
            }
            Event::Rule => {
                blocks.push(MdBlock::HorizontalRule);
                i += 1;
            }
            _ => {
                i += 1;
            }
        }
    }

    blocks
}

pub fn spawn_markdown_blocks(parent: Entity, blocks: &[MdBlock], commands: &mut Commands) {
    let text_color = Color::srgb(0.75, 0.8, 0.85);
    let heading_color = Color::srgb(0.3, 0.6, 0.9);
    let dim_color = Color::srgb(0.4, 0.45, 0.5);
    let quote_color = Color::srgb(0.55, 0.62, 0.72);
    let code_bg = Color::srgba(0.04, 0.05, 0.07, 0.6);

    commands.entity(parent).with_children(|parent| {
        for block in blocks {
            match block {
                MdBlock::Heading { level, text } => {
                    let size = match level {
                        1 => 16.0,
                        2 => 14.0,
                        3 => 12.5,
                        _ => 12.0,
                    };
                    let mt = match level {
                        1 => 14.0,
                        2 => 10.0,
                        _ => 6.0,
                    };
                    parent.spawn((
                        Node {
                            margin: UiRect::top(Val::Px(mt)),
                            ..default()
                        },
                        Text::new(text.clone()),
                        TextFont {
                            font_size: size,
                            ..default()
                        },
                        TextColor(heading_color),
                    ));
                }
                MdBlock::Paragraph { text } => {
                    parent.spawn((
                        Text::new(text.clone()),
                        TextFont {
                            font_size: 11.5,
                            ..default()
                        },
                        TextColor(text_color),
                    ));
                }
                MdBlock::BlockQuote { text } => {
                    parent
                        .spawn((
                            Node {
                                border: UiRect::left(Val::Px(3.0)),
                                padding: UiRect::new(
                                    Val::Px(10.0),
                                    Val::Px(10.0),
                                    Val::Px(4.0),
                                    Val::Px(4.0),
                                ),
                                ..default()
                            },
                            BorderColor::all(Color::srgba(0.3, 0.6, 0.9, 0.4)),
                            BackgroundColor(Color::srgba(0.3, 0.6, 0.9, 0.06)),
                        ))
                        .with_children(|q| {
                            q.spawn((
                                Text::new(text.clone()),
                                TextFont {
                                    font_size: 11.0,
                                    ..default()
                                },
                                TextColor(quote_color),
                            ));
                        });
                }
                MdBlock::CodeBlock { text, .. } => {
                    let lines = text.lines().count().max(1) as f32;
                    let h = 12.0 * lines + 8.0;
                    parent
                        .spawn((
                            Node {
                                padding: UiRect::all(Val::Px(8.0)),
                                width: Val::Percent(100.0),
                                height: Val::Px(h.max(28.0)),
                                ..default()
                            },
                            BackgroundColor(code_bg),
                            BorderRadius::all(Val::Px(4.0)),
                        ))
                        .with_children(|c| {
                            c.spawn((
                                Text::new(text.clone()),
                                TextFont {
                                    font_size: 10.0,
                                    ..default()
                                },
                                TextColor(Color::srgb(0.5, 0.75, 0.5)),
                                Node {
                                    width: Val::Percent(100.0),
                                    ..default()
                                },
                            ));
                        });
                }
                MdBlock::UnorderedList { items } => {
                    parent
                        .spawn(Node {
                            flex_direction: FlexDirection::Column,
                            row_gap: Val::Px(3.0),
                            ..default()
                        })
                        .with_children(|list| {
                            for item in items {
                                list.spawn(Node {
                                    flex_direction: FlexDirection::Row,
                                    column_gap: Val::Px(6.0),
                                    ..default()
                                })
                                .with_children(|row| {
                                    row.spawn((
                                        Text::new("\u{2022} ".to_string() + item),
                                        TextFont {
                                            font_size: 11.0,
                                            ..default()
                                        },
                                        TextColor(text_color),
                                    ));
                                });
                            }
                        });
                }
                MdBlock::HorizontalRule => {
                    parent.spawn((
                        Node {
                            width: Val::Percent(100.0),
                            height: Val::Px(1.0),
                            margin: UiRect::vertical(Val::Px(8.0)),
                            ..default()
                        },
                        BackgroundColor(Color::srgba(0.3, 0.3, 0.4, 0.3)),
                    ));
                }
            }
        }
    });
}
