//! Rendering.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Cell, Gauge, Paragraph, Row, Table, Tabs, Wrap};

use crate::app::{App, Mode};
use crate::chronicle;
use crate::items::{Item, Kind};

const SET_GREEN: Color = Color::Rgb(0x00, 0xC4, 0x00);
const UNIQUE_GOLD: Color = Color::Rgb(0xC7, 0xB3, 0x77);
const RUNEWORD_ORANGE: Color = Color::Rgb(0xFF, 0xA8, 0x00);
/// The game's blue for item stats, lightened a little to read on dark terminals.
const STAT_BLUE: Color = Color::Rgb(0x80, 0x80, 0xFF);

/// Narrowest the right side (progress and details) gets; the item table yields first.
const MIN_SIDE_WIDTH: u16 = 44;
/// Space between table columns.
const COLUMN_SPACING: u16 = 2;
const HIGHLIGHT_SYMBOL: &str = "▶ ";
/// Width of the Found column (a date).
const FOUND_WIDTH: u16 = 10;

const fn kind_color(kind: Kind) -> Color {
    match kind {
        Kind::Set => SET_GREEN,
        Kind::Unique => UNIQUE_GOLD,
        Kind::Runeword => RUNEWORD_ORANGE,
    }
}

pub fn draw(f: &mut Frame, app: &mut App) {
    let [tabs, main, status, help] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(f.area());
    // The table is as wide as it gets in its widest tab, so it doesn't change size between tabs,
    // and the right side takes the rest.
    let table_width = std::iter::once(None)
        .chain(Kind::ALL.map(Some))
        .map(|kind| table_width(app, kind))
        .max()
        .unwrap_or(0)
        .min(main.width.saturating_sub(MIN_SIDE_WIDTH));
    let [left, right] =
        Layout::horizontal([Constraint::Length(table_width), Constraint::Fill(1)]).areas(main);
    let [progress, details] =
        Layout::vertical([Constraint::Length(10), Constraint::Min(0)]).areas(right);

    draw_tabs(f, app, tabs);
    draw_table(f, app, left);
    draw_progress(f, app, progress);
    draw_details(f, app, details);
    f.render_widget(Line::from(app.status.as_str()).yellow(), status);
    draw_help(f, app, help);
}

fn draw_tabs(f: &mut Frame, app: &App, area: Rect) {
    let [area, indicator] =
        Layout::horizontal([Constraint::Min(0), Constraint::Length(10)]).areas(area);
    let live = if app.watching {
        "● live".green()
    } else {
        "○ polling".yellow()
    };
    f.render_widget(Line::from(live).right_aligned(), indicator);

    let titles = std::iter::once(None)
        .chain(Kind::ALL.map(Some))
        .map(|kind| {
            let label = kind.map_or("All", Kind::label);
            Line::from(format!("{label} {}", app.remaining(kind)))
        });
    f.render_widget(
        Tabs::new(titles)
            .select(app.tab)
            .highlight_style(Style::new().add_modifier(Modifier::REVERSED | Modifier::BOLD))
            .divider("│"),
        area,
    );
}

/// A text column of the item table: its title and the item field it shows.
type Column = (&'static str, fn(&Item) -> &str);

fn columns(kind: Option<Kind>) -> Vec<Column> {
    let base_title = match kind {
        None => "Base / Runes",
        Some(Kind::Runeword) => "Runes",
        Some(_) => "Base",
    };
    let mut cols: Vec<Column> = vec![("Item", |i| i.name), (base_title, Item::base_or_runes)];
    if kind.is_none_or(|k| k == Kind::Runeword) {
        cols.push(("Top rune", |i| &i.top_rune_label));
    }
    if kind != Some(Kind::Runeword) {
        cols.push(("Type", |i| i.item_type));
        cols.push(("Tier", Item::tier_label));
        cols.push(("QLvl", |i| i.qlvl));
    }
    if kind.is_none_or(|k| k == Kind::Set) {
        cols.push(("Set", |i| i.set));
    }
    cols
}

/// Widths of the table's columns in a tab: each fits its longest entry in that tab (not just the
/// visible rows, so the columns don't jump around while searching or filtering), plus Found.
fn column_widths(app: &App, kind: Option<Kind>) -> Vec<u16> {
    let in_tab: Vec<_> = app
        .items
        .iter()
        .filter(|i| kind.is_none_or(|k| i.kind == k))
        .collect();
    let mut widths: Vec<u16> = columns(kind)
        .iter()
        .map(|&(title, field)| {
            let longest = in_tab
                .iter()
                .map(|&i| field(i).chars().count())
                .max()
                .unwrap_or(0);
            u16::try_from(longest.max(title.len())).unwrap_or(u16::MAX)
        })
        .collect();
    if app.show_found {
        widths.push(FOUND_WIDTH);
    }
    widths
}

/// The table's full width in a tab: its columns and the spaces between them, the highlight
/// symbol and the border.
fn table_width(app: &App, kind: Option<Kind>) -> u16 {
    let widths = column_widths(app, kind);
    let gaps = u16::try_from(widths.len().saturating_sub(1)).unwrap_or(u16::MAX);
    let symbol = u16::try_from(HIGHLIGHT_SYMBOL.chars().count()).unwrap_or(u16::MAX);
    widths
        .iter()
        .fold(0u16, |sum, &w| sum.saturating_add(w))
        .saturating_add(gaps.saturating_mul(COLUMN_SPACING))
        .saturating_add(symbol)
        .saturating_add(2)
}

fn draw_table(f: &mut Frame, app: &mut App, area: Rect) {
    let kind = app.tab_kind();
    let cols = columns(kind);
    let visible = app.visible();

    let rows = visible.iter().map(|&i| {
        let item = &app.items[i];
        let found = app.found[i];
        let mut cells: Vec<Cell> = cols
            .iter()
            .map(|&(_, field)| Cell::from(field(item)))
            .collect();
        if app.show_found {
            cells.push(Cell::from(
                found.map(chronicle::fmt_date).unwrap_or_default(),
            ));
        }
        let style = if found.is_some() {
            Style::new().dark_gray()
        } else {
            Style::new().fg(kind_color(item.kind))
        };
        Row::new(cells).style(style)
    });

    let widths = column_widths(app, kind).into_iter().map(Constraint::Length);
    let mut header: Vec<&str> = cols.iter().map(|&(title, _)| title).collect();
    if app.show_found {
        header.push("Found");
    }

    let what = if app.show_found {
        "All items"
    } else {
        "Remaining"
    };
    let mut title = format!(" {what} ({}) ", visible.len());
    if let Some(tier) = app.tier {
        title = format!("{title}· {} only ", tier.label());
    }
    if !app.search.is_empty() {
        title = format!("{title}· matching \"{}\" ", app.search);
    }
    let table = Table::new(rows, widths)
        .header(Row::new(header).bold())
        .column_spacing(COLUMN_SPACING)
        .block(Block::bordered().title(title))
        .row_highlight_style(Style::new().add_modifier(Modifier::REVERSED | Modifier::BOLD))
        .highlight_symbol(HIGHLIGHT_SYMBOL);
    app.table_state
        .select((!visible.is_empty()).then_some(app.row));
    f.render_stateful_widget(table, area, &mut app.table_state);
}

fn draw_progress(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::bordered().title(" Chronicle progress ");
    let inner = block.inner(area);
    f.render_widget(block, area);

    let kinds = std::iter::once(None).chain(Kind::ALL.map(Some));
    let areas = Layout::vertical([Constraint::Length(2); 4]).split(inner);
    for (kind, &area) in kinds.zip(areas.iter()) {
        let [label, gauge] = Layout::vertical([Constraint::Length(1); 2]).areas(area);
        let (done, total) = app.progress(kind);
        let name = kind.map_or("Total", Kind::label);
        let color = kind.map_or(Color::Cyan, kind_color);
        f.render_widget(
            Line::from(vec![
                Span::from(format!("{name:<10}")).fg(color).bold(),
                format!("{done}/{total} found, ").into(),
                format!("{} left", total - done).bold(),
            ]),
            label,
        );
        let percent = (done * 100).checked_div(total).unwrap_or(0);
        f.render_widget(
            Gauge::default()
                .gauge_style(Style::new().fg(color).bg(Color::DarkGray))
                .percent(u16::try_from(percent).unwrap_or(100)),
            gauge,
        );
    }
}

fn draw_details(f: &mut Frame, app: &mut App, area: Rect) {
    let paragraph = Paragraph::new(detail_lines(app)).wrap(Wrap { trim: false });
    // Stop once the last (wrapped) line reaches the bottom of the pane, inside its border.
    let wrapped = paragraph.line_count(area.width.saturating_sub(2));
    let max_scroll = wrapped.saturating_sub(usize::from(area.height.saturating_sub(2)));
    app.detail_scroll = app
        .detail_scroll
        .min(u16::try_from(max_scroll).unwrap_or(u16::MAX));
    let title = if app.detail_scroll > 0 {
        " Details (scrolled, k up) "
    } else {
        " Details "
    };
    f.render_widget(
        paragraph
            .scroll((app.detail_scroll, 0))
            .block(Block::bordered().title(title)),
        area,
    );
}

fn detail_lines(app: &App) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    match app.selected() {
        None if app.visible().is_empty() && !app.show_found && app.search.is_empty() => {
            lines.push(
                "✓ Everything here is in your Chronicle!"
                    .green()
                    .bold()
                    .into(),
            );
        }
        None => lines.push("No items match.".dark_gray().into()),
        Some(i) => {
            let item = &app.items[i];
            let d = &item.details;
            let field = |label: &'static str, value: &'static str| {
                Line::from(vec![label.dark_gray(), Span::from(value).bold()])
            };
            lines.push(Line::from(item.name).fg(kind_color(item.kind)).bold());
            if item.kind == Kind::Runeword {
                lines.push(field("Runes: ", item.runes));
                lines.push(Line::from(vec![
                    "Highest rune: ".dark_gray(),
                    Span::from(item.top_rune_label.clone()).bold(),
                ]));
                lines.push(Line::from(vec![
                    "Bases: ".dark_gray(),
                    Span::from(d.bases).bold(),
                    format!(" · {} sockets", d.sockets).into(),
                ]));
            } else {
                let tier = item
                    .tier
                    .map_or(String::new(), |t| format!("{} ", t.label()));
                lines.push(Line::from(format!(
                    "{} · {tier}{}",
                    item.base, item.item_type
                )));
                lines.push(field("Quality level: ", item.qlvl));
            }
            lines.push(field("Required level: ", d.req_level));
            lines.push(match app.found[i] {
                Some(t) => Line::from(format!("✓ Found {}", chronicle::fmt_date(t))).green(),
                None => "✗ Not found yet".red().into(),
            });
            lines.push(Line::from(""));
            lines.extend(d.stats.iter().map(|&s| Line::from(s).fg(STAT_BLUE)));
            lines.extend(d.set_bonuses.iter().map(|&s| Line::from(s).fg(SET_GREEN)));
            if !item.set.is_empty() {
                lines.push(Line::from(""));
                lines.extend(set_lines(app, item.set));
            }
        }
    }
    if !app.unknown.is_empty() {
        lines.push(Line::from(""));
        lines.push(
            format!(
                "{} Chronicle entries match no known item",
                app.unknown.len()
            )
            .dark_gray()
            .into(),
        );
    }
    lines
}

fn set_lines(app: &App, set: &'static str) -> Vec<Line<'static>> {
    let pieces: Vec<usize> = (0..app.items.len())
        .filter(|&i| app.items[i].set == set)
        .collect();
    let done = pieces.iter().filter(|&&i| app.found[i].is_some()).count();
    let mut lines = vec![
        Line::from(format!("{set} ({done}/{})", pieces.len()))
            .fg(SET_GREEN)
            .bold(),
    ];
    for i in pieces {
        let item = &app.items[i];
        lines.push(if app.found[i].is_some() {
            Line::from(format!(" ✓ {}", item.name)).dark_gray()
        } else {
            Line::from(format!(" · {}", item.name))
        });
    }
    if let Some(bonuses) = app.sets.get(set) {
        lines.extend(bonuses.partial.iter().map(|&s| Line::from(s).fg(SET_GREEN)));
        if !bonuses.full.is_empty() {
            lines.push(Line::from("Full set:").fg(UNIQUE_GOLD).bold());
            lines.extend(
                bonuses
                    .full
                    .iter()
                    .map(|&s| Line::from(format!(" {s}")).fg(UNIQUE_GOLD)),
            );
        }
    }
    lines
}

fn draw_help(f: &mut Frame, app: &App, area: Rect) {
    let line = match app.mode {
        Mode::Search => Line::from(vec![
            "Search: ".bold(),
            format!("{}_", app.search).into(),
            "   (Enter to keep, Esc to clear)".dark_gray(),
        ]),
        Mode::Normal => Line::from(
            "↑↓ move  ←→/Tab/1-4 category  t tier  / search  f show found  j/k scroll details  r reload  q quit",
        )
        .dark_gray(),
    };
    f.render_widget(line, area);
}

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::*;
    use crate::chronicle::Entry;

    fn render(app: &mut App) -> String {
        render_at(app, 120)
    }

    fn render_at(app: &mut App, width: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, 32)).unwrap();
        terminal.draw(|f| draw(f, app)).unwrap();
        let buf = terminal.backend().buffer();
        (0..buf.area.height)
            .map(|y| {
                (0..buf.area.width)
                    .map(|x| buf[(x, y)].symbol())
                    .collect::<String>()
                    + "\n"
            })
            .collect()
    }

    #[test]
    fn shows_counts_and_set_pieces() {
        let blank = App::new(&[]);
        let i = blank
            .items
            .iter()
            .position(|i| i.name == "Tal Rasha's Guardianship")
            .unwrap();
        let entry = Entry {
            kind: Kind::Set,
            row: blank.items[i].row,
            found_at: 0,
        };
        let mut app = App::new(&[entry]);
        app.search = "tal rasha".into();
        let screen = render(&mut app);
        println!("{screen}");
        assert!(screen.contains("All 636"));
        assert!(screen.contains("1/135 found, 134 left"));
        assert!(screen.contains("Tal Rasha's Wrappings (1/5)"));
        assert!(screen.contains("✓ Tal Rasha's Guardianship"));
    }

    #[test]
    fn shows_stats_and_scrolls_to_set_bonuses() {
        let mut app = App::new(&[]);
        app.search = "enigma".into();
        let screen = render(&mut app);
        assert!(screen.contains("Required level: 65"));
        assert!(screen.contains("Bases: Body Armor · 3 sockets"));
        assert!(screen.contains("+1 to Teleport"));

        app.search = "tal rasha's guardianship".into();
        app.detail_scroll = 20;
        let screen = render(&mut app);
        println!("{screen}");
        assert!(screen.contains("Full set:"));
        assert!(screen.contains("All Resistances +50"));
    }

    #[test]
    fn details_scroll_stops_with_the_last_line_at_the_bottom() {
        let mut app = App::new(&[]);
        app.search = "tal rasha's guardianship".into();
        app.detail_scroll = u16::MAX;
        let screen = render(&mut app);
        println!("{screen}");
        assert!(app.detail_scroll > 0 && app.detail_scroll < u16::MAX);
        let last_line = detail_lines(&app).pop().unwrap().to_string();
        let last_word = last_line.split_whitespace().last().unwrap();
        // The pane's bottom border sits above the status and help lines; the row above it is the
        // pane's last row.
        let rows: Vec<&str> = screen.lines().collect();
        let bottom_row = rows[rows.len() - 4];
        assert!(
            bottom_row.contains(last_word),
            "{last_word:?} not in {bottom_row:?}"
        );
    }

    #[test]
    fn table_takes_its_widest_tab_and_the_side_the_rest() {
        let mut app = App::new(&[]);
        let widest = (0..4)
            .map(|tab| {
                app.tab = tab;
                table_width(&app, app.tab_kind())
            })
            .max()
            .unwrap();
        // The longest set name, shown in full in the All tab, whose table is the widest.
        let set = app
            .items
            .iter()
            .map(|i| i.set)
            .max_by_key(|s| s.len())
            .unwrap();
        app.search = set.to_lowercase();
        for tab in 0..4 {
            app.tab = tab;
            let screen = render_at(&mut app, 220);
            println!("{screen}");
            // The table's top-right corner, then the progress pane's top-left one.
            let top: Vec<char> = screen.lines().nth(1).unwrap().chars().collect();
            assert_eq!(top[usize::from(widest) - 1], '┐');
            assert_eq!(top[usize::from(widest)], '┌');
            if tab == 0 {
                assert!(screen.contains(&format!("{set} ")));
            }
        }
        // Narrow terminals keep the side at its minimum width.
        let screen = render_at(&mut app, 100);
        let top: Vec<char> = screen.lines().nth(1).unwrap().chars().collect();
        assert_eq!(top[usize::from(100 - MIN_SIDE_WIDTH)], '┌');
    }
}
