//! The Stats page: what Synapse saved, from `synapse savings`, and what every
//! session carries, from `synapse tokens`.

use crate::ui::settings::{Click, option};
use gpui::prelude::*;
use gpui::{AnyElement, App, Hsla, div, px};
use guise::prelude::*;
use synapsecore::usage::{self, Report, Surface, approximately, money, thousands, tokens};

/// `None` is all time.
const WINDOWS: [(Option<u32>, &str); 3] = [
    (Some(7), "7 days"),
    (Some(30), "30 days"),
    (None, "All time"),
];

/// guise's y-axis width. The plot takes the full width and the axis sits
/// beside it, so the panel has to leave this much room or the plot overflows.
const AXIS: f32 = 36.0;

/// Dollars per million input tokens.
const PRICES: [(f64, &str, &str); 4] = [
    (0.5, "$0.50", "Small, fast models"),
    (2.0, "$2", "Mid-tier models · the default"),
    (4.0, "$4", "Top-tier models"),
    (10.0, "$10", "The largest models"),
];

pub struct View {
    pub report: Option<Report>,
    pub surfaces: Vec<Surface>,
    pub days: Option<u32>,
    pub error: Option<String>,
}

pub struct Actions {
    pub window: Box<dyn Fn(Option<u32>) -> Click>,
    pub price: Box<dyn Fn(f64) -> Click>,
    pub refresh: Click,
}

pub fn render(view: View, actions: Actions, cx: &App) -> AnyElement {
    let theme = guise::theme(cx);
    let border = theme.border().hsla();
    let surface = theme.surface().hsla();
    let Actions {
        window,
        price,
        refresh,
    } = actions;

    let mut page = div()
        .w_full()
        .max_w(px(980.0))
        .mx_auto()
        .px(px(34.0))
        .py(px(28.0))
        .flex()
        .flex_col()
        .gap(px(20.0))
        .child(header(view.days, &window, refresh));

    if let Some(error) = view.error {
        page = page.child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .text_color(theme.danger().hsla())
                .child(Icon::new(IconName::CircleAlert).size(Size::Sm))
                .child(Text::new(error).size(Size::Sm)),
        );
    }

    let warning = theme.warning().hsla();
    let carried =
        (!view.surfaces.is_empty()).then(|| persession(&view.surfaces, border, surface, warning));
    page = match view.report {
        Some(report) if report.since.is_some() => page
            .child(tiles(&report, border, surface))
            .child(chart(&report, border, surface))
            .child(breakdown(&report, border, surface))
            .children(carried)
            .child(pricing(report.price, &price, border, surface))
            .child(footnote(&report)),
        Some(report) => page
            .child(empty(border, surface))
            .children(carried)
            .child(pricing(report.price, &price, border, surface)),
        None => page.children(carried),
    };

    div()
        .id("statsmain")
        .flex_1()
        .min_h(px(0.0))
        .overflow_y_scroll()
        .child(page)
        .into_any_element()
}

fn header(days: Option<u32>, window: &dyn Fn(Option<u32>) -> Click, refresh: Click) -> AnyElement {
    div()
        .flex()
        .items_start()
        .justify_between()
        .gap(px(24.0))
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .flex()
                .flex_col()
                .gap(px(7.0))
                .child(Title::new("Stats").order(2))
                .child(
                    Text::new(
                        "Tokens kept out of your sessions, compared with loading the same memory from an instruction file, and what each session carries.",
                    )
                    .size(Size::Sm)
                    .dimmed(),
                ),
        )
        .child(
            div()
                .flex_none()
                .flex()
                .items_center()
                .gap(px(6.0))
                .children(WINDOWS.into_iter().map(|(choice, label)| {
                    let click = window(choice);
                    Button::new(
                        gpui::ElementId::Name(format!("statswindow{label}").into()),
                        label,
                    )
                    .variant(if choice == days {
                        Variant::Filled
                    } else {
                        Variant::Subtle
                    })
                    .color(ColorName::Violet)
                    .size(Size::Sm)
                    .on_click(move |event, window, cx| click(event, window, cx))
                }))
                .child(
                    Button::new("statsrefresh", "Refresh")
                        .variant(Variant::Subtle)
                        .color(ColorName::Violet)
                        .size(Size::Sm)
                        .left_section(Icon::new(IconName::RefreshCw).size(Size::Xs))
                        .on_click(move |event, window, cx| refresh(event, window, cx)),
                ),
        )
        .into_any_element()
}

fn tiles(report: &Report, border: Hsla, surface: Hsla) -> AnyElement {
    let totals = &report.totals;
    let net = tokens(totals.net());
    div()
        .flex()
        .gap(px(12.0))
        .child(tile(
            "Net tokens saved",
            approximately(net),
            "after what Synapse itself costs".to_owned(),
            border,
            surface,
        ))
        .child(tile(
            "Worth",
            money(usage::dollars(net, report.price)),
            format!("at ${:.2} per million input tokens", report.price),
            border,
            surface,
        ))
        .child(tile(
            "Sessions",
            thousands(totals.sessions as usize),
            format!(
                "{} memories handed over",
                thousands(totals.memories as usize)
            ),
            border,
            surface,
        ))
        .child(tile(
            "Recalls",
            thousands(totals.recalls as usize),
            format!("{} pages read back", thousands(totals.reads as usize)),
            border,
            surface,
        ))
        .into_any_element()
}

fn tile(label: &str, value: String, detail: String, border: Hsla, surface: Hsla) -> AnyElement {
    div()
        .flex_1()
        .min_w(px(0.0))
        .rounded(px(6.0))
        .border_1()
        .border_color(border)
        .bg(surface)
        .p(px(16.0))
        .flex()
        .flex_col()
        .gap(px(4.0))
        .child(Text::new(label.to_owned()).size(Size::Xs).dimmed())
        .child(Title::new(value).order(3))
        .child(Text::new(detail).size(Size::Xs).dimmed())
        .into_any_element()
}

fn chart(report: &Report, border: Hsla, surface: Hsla) -> AnyElement {
    let values = report.daily.iter().map(|day| tokens(day.net).max(0) as f32);
    // Thirty labels won't fit, so longer windows just label both ends.
    let labelled = report.daily.len() <= 7;
    let bars = match labelled {
        true => BarChart::entries(
            report
                .daily
                .iter()
                .map(|day| (short(&day.date), tokens(day.net).max(0) as f32)),
        ),
        false => BarChart::new(values),
    }
    .color(ColorName::Violet)
    .axis()
    .hover()
    .height(160.0);
    let losing = report.daily.iter().any(|day| day.net < 0);
    let mut panel = div()
        .rounded(px(6.0))
        .border_1()
        .border_color(border)
        .bg(surface)
        .p(px(16.0))
        .flex()
        .flex_col()
        .gap(px(12.0))
        .child(Text::new("Net tokens saved per day").size(Size::Sm).bold())
        .child(div().flex().flex_col().pr(px(AXIS)).child(bars));
    if !labelled && let (Some(first), Some(last)) = (report.daily.first(), report.daily.last()) {
        panel = panel.child(
            div()
                .flex()
                .justify_between()
                .pl(px(AXIS))
                .child(Text::new(short(&first.date)).size(Size::Xs).dimmed())
                .child(Text::new(short(&last.date)).size(Size::Xs).dimmed()),
        );
    }
    if losing {
        panel = panel.child(
            Text::new("A day Synapse cost more than it kept out shows no bar.")
                .size(Size::Xs)
                .dimmed(),
        );
    }
    panel.into_any_element()
}

fn breakdown(report: &Report, border: Hsla, surface: Hsla) -> AnyElement {
    let totals = &report.totals;
    let mut panel = div()
        .rounded(px(6.0))
        .border_1()
        .border_color(border)
        .bg(surface)
        .p(px(16.0))
        .flex()
        .flex_col()
        .gap(px(10.0))
        .child(Text::new("How it adds up").size(Size::Sm).bold())
        .child(line(
            "Memory in reach",
            totals.reach,
            "Cost of loading it all from an instruction file, per session".to_owned(),
        ))
        .child(line(
            "Handed over",
            -totals.sent,
            format!(
                "Recalled into context, ~{} tokens of it read back",
                thousands(tokens(totals.readback) as usize)
            ),
        ))
        .child(line(
            "Synapse's own",
            -totals.overhead,
            "Tool definitions, its own guidance, and response framing".to_owned(),
        ))
        .child(div().h(px(1.0)).bg(border))
        .child(line(
            "Net saved",
            totals.net(),
            "Kept out of context, minus Synapse's own cost".to_owned(),
        ));
    if totals.trimmed > 0 {
        panel = panel.child(
            Text::new(format!(
                "Recall's budget kept ~{} tokens of matched memory out of context by abridging and deduplicating it.",
                thousands(tokens(totals.trimmed) as usize)
            ))
            .size(Size::Xs)
            .dimmed(),
        );
    }
    panel.into_any_element()
}

fn line(label: &'static str, chars: i64, detail: String) -> AnyElement {
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(16.0))
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .flex()
                .flex_col()
                .gap(px(2.0))
                .child(Text::new(label).size(Size::Sm))
                .child(Text::new(detail).size(Size::Xs).dimmed()),
        )
        .child(
            div().flex_none().child(
                Text::new(format!("{} tokens", approximately(tokens(chars))))
                    .size(Size::Sm)
                    .bold(),
            ),
        )
        .into_any_element()
}

fn persession(surfaces: &[Surface], border: Hsla, background: Hsla, warning: Hsla) -> AnyElement {
    let total: usize = surfaces.iter().map(|item| item.chars).sum();
    let mut panel = div()
        .rounded(px(6.0))
        .border_1()
        .border_color(border)
        .bg(background)
        .p(px(16.0))
        .flex()
        .flex_col()
        .gap(px(10.0))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(2.0))
                .child(Text::new("In every session").size(Size::Sm).bold())
                .child(
                    Text::new(
                        "Loaded before the first turn by each connected tool. Same numbers as `synapse tokens`.",
                    )
                    .size(Size::Xs)
                    .dimmed(),
                ),
        )
        .children(surfaces.iter().map(|item| {
            line(
                item.label,
                item.chars as i64,
                format!("{} · {} chars", item.detail, thousands(item.chars)),
            )
        }))
        .child(div().h(px(1.0)).bg(border))
        .child(line(
            "Total",
            total as i64,
            "Per session, per connected tool".to_owned(),
        ));
    for text in surfaces.iter().filter_map(|item| item.warning.clone()) {
        panel = panel.child(
            div()
                .flex()
                .items_start()
                .gap(px(8.0))
                .text_color(warning)
                .child(
                    div()
                        .flex_none()
                        .child(Icon::new(IconName::TriangleAlert).size(Size::Xs)),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .child(Text::new(text).size(Size::Xs)),
                ),
        );
    }
    panel.into_any_element()
}

fn pricing(current: f64, price: &dyn Fn(f64) -> Click, border: Hsla, surface: Hsla) -> AnyElement {
    div()
        .rounded(px(6.0))
        .border_1()
        .border_color(border)
        .bg(surface)
        .p(px(16.0))
        .flex()
        .flex_col()
        .gap(px(13.0))
        .child(
            div()
                .flex()
                .items_start()
                .justify_between()
                .gap(px(24.0))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .flex()
                        .flex_col()
                        .gap(px(4.0))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .child(Icon::new(IconName::CircleDollarSign).size(Size::Sm))
                                .child(Text::new("Price per million input tokens").size(Size::Sm).bold()),
                        )
                        .child(
                            Text::new(
                                "What the dollar figure is worked out at. Pick the rate of the model you use most.",
                            )
                            .size(Size::Xs)
                            .dimmed(),
                        ),
                )
                .child(Badge::new(format!("${current:.2}")).color(ColorName::Violet)),
        )
        .child(
            div()
                .flex()
                .gap(px(10.0))
                .children(PRICES.into_iter().map(|(rate, label, description)| {
                    option(
                        "price",
                        label,
                        description,
                        (current - rate).abs() < f64::EPSILON,
                        price(rate),
                    )
                    .into_any_element()
                })),
        )
        .child(
            Text::new("`synapse settings price <dollars>` takes any rate.")
                .size(Size::Xs)
                .dimmed(),
        )
        .into_any_element()
}

fn empty(border: Hsla, surface: Hsla) -> AnyElement {
    div()
        .rounded(px(6.0))
        .border_1()
        .border_color(border)
        .bg(surface)
        .p(px(22.0))
        .flex()
        .flex_col()
        .items_start()
        .gap(px(10.0))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(Icon::new(IconName::PiggyBank).size(Size::Sm))
                .child(Text::new("Nothing counted yet").size(Size::Sm).bold()),
        )
        .child(
            Text::new("Open a session in a connected tool and it'll show up here.")
                .size(Size::Xs)
                .dimmed(),
        )
        .into_any_element()
}

fn footnote(report: &Report) -> AnyElement {
    let since = report.since.as_deref().unwrap_or_default();
    Text::new(format!(
        "Counting since {since} on this machine. Usage older than {} days is pruned. Tokens are characters ÷ {}, counted once per session, so treat these as a floor.",
        usage::RETENTIONDAYS,
        usage::PERTOKEN,
    ))
    .size(Size::Xs)
    .dimmed()
    .into_any_element()
}

/// `2026-10-19` as `Oct 19`.
fn short(date: &str) -> String {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let mut parts = date.split('-').skip(1);
    let month = parts
        .next()
        .and_then(|value| value.parse::<usize>().ok())
        .and_then(|value| MONTHS.get(value.wrapping_sub(1)));
    let day = parts.next().and_then(|value| value.parse::<u32>().ok());
    match (month, day) {
        (Some(month), Some(day)) => format!("{month} {day}"),
        _ => date.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_date_is_shortened_to_month_and_day() {
        assert_eq!(short("2026-10-19"), "Oct 19");
        assert_eq!(short("2026-01-05"), "Jan 5");
        assert_eq!(short("not a date"), "not a date");
        assert_eq!(short("2026-13-01"), "2026-13-01");
    }
}
