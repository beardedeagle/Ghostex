//! What the toolbar narrows and orders the board by, and which card details are shown. Ported
//! from `filterBoardTickets`, `sortBoardTickets` and card-view-options.ts.

use super::model::{BoardTicket, estimate_to_tshirt, parse_time_ms, priority_select_value};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum KanbanSort {
    Default,
    UpdatedDesc,
    UpdatedAsc,
    CreatedDesc,
    CreatedAsc,
    PriorityAsc,
    PriorityDesc,
}

impl KanbanSort {
    pub(crate) const ALL: [KanbanSort; 7] = [
        KanbanSort::Default,
        KanbanSort::UpdatedDesc,
        KanbanSort::UpdatedAsc,
        KanbanSort::CreatedDesc,
        KanbanSort::CreatedAsc,
        KanbanSort::PriorityAsc,
        KanbanSort::PriorityDesc,
    ];

    pub(crate) fn id(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::UpdatedDesc => "updated-desc",
            Self::UpdatedAsc => "updated-asc",
            Self::CreatedDesc => "created-desc",
            Self::CreatedAsc => "created-asc",
            Self::PriorityAsc => "priority-asc",
            Self::PriorityDesc => "priority-desc",
        }
    }

    pub(crate) fn from_id(id: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|sort| sort.id() == id)
            .unwrap_or(Self::Default)
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Default => "Default order",
            Self::UpdatedDesc => "Last updated (newest first)",
            Self::UpdatedAsc => "Last updated (oldest first)",
            Self::CreatedDesc => "Created (newest first)",
            Self::CreatedAsc => "Created (oldest first)",
            Self::PriorityAsc => "Priority (urgent first)",
            Self::PriorityDesc => "Priority (low first)",
        }
    }
}

/// The toolbar's filter and sort selections. `"all"` means no filter.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct KanbanViewPreferences {
    pub(crate) priority: String,
    pub(crate) estimate: String,
    pub(crate) tag: String,
    pub(crate) sort: KanbanSort,
}

impl Default for KanbanViewPreferences {
    fn default() -> Self {
        Self {
            priority: "all".to_string(),
            estimate: "all".to_string(),
            tag: "all".to_string(),
            sort: KanbanSort::Default,
        }
    }
}

impl KanbanViewPreferences {
    /// The tag filter as it applies to the loaded board: a stored tag no ticket carries reads as
    /// "all" without being overwritten (`resolveBoardTagFilter`).
    pub(crate) fn active_tag<'a>(&'a self, tags: &[String]) -> &'a str {
        if self.tag != "all" && tags.contains(&self.tag) {
            &self.tag
        } else {
            "all"
        }
    }

    pub(crate) fn active_count(&self, tags: &[String]) -> usize {
        usize::from(self.priority != "all")
            + usize::from(self.estimate != "all")
            + usize::from(self.active_tag(tags) != "all")
            + usize::from(self.sort != KanbanSort::Default)
    }
}

/// `BoardCardViewOptions`: which card details are drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct KanbanCardView {
    pub(crate) show_id: bool,
    pub(crate) show_priority: bool,
    pub(crate) show_assignee: bool,
    pub(crate) show_description: bool,
    pub(crate) show_labels: bool,
    pub(crate) show_details: bool,
    pub(crate) show_links: bool,
}

impl Default for KanbanCardView {
    fn default() -> Self {
        Self {
            show_id: true,
            show_priority: true,
            show_assignee: true,
            show_description: false,
            show_labels: true,
            show_details: true,
            show_links: true,
        }
    }
}

/// `BOARD_CARD_VIEW_FIELDS`, in menu order.
pub(crate) const CARD_VIEW_FIELDS: [(&str, &str); 7] = [
    ("showId", "Ticket ID"),
    ("showPriority", "Priority"),
    ("showAssignee", "Assignee"),
    ("showDescription", "Description"),
    ("showLabels", "Labels"),
    ("showDetails", "Details"),
    ("showLinks", "Conversation links"),
];

impl KanbanCardView {
    pub(crate) fn field(&self, key: &str) -> bool {
        match key {
            "showId" => self.show_id,
            "showPriority" => self.show_priority,
            "showAssignee" => self.show_assignee,
            "showDescription" => self.show_description,
            "showLabels" => self.show_labels,
            "showDetails" => self.show_details,
            "showLinks" => self.show_links,
            _ => false,
        }
    }

    pub(crate) fn toggle(&mut self, key: &str) {
        let slot = match key {
            "showId" => &mut self.show_id,
            "showPriority" => &mut self.show_priority,
            "showAssignee" => &mut self.show_assignee,
            "showDescription" => &mut self.show_description,
            "showLabels" => &mut self.show_labels,
            "showDetails" => &mut self.show_details,
            "showLinks" => &mut self.show_links,
            _ => return,
        };
        *slot = !*slot;
    }
}

/// `boardTagFilterOptions` without the leading "all": the labels the loaded tickets carry.
pub(crate) fn tag_options(tickets: &[BoardTicket]) -> Vec<String> {
    let mut tags = tickets
        .iter()
        .flat_map(|ticket| ticket.issue.labels.iter().cloned())
        .collect::<Vec<_>>();
    tags.sort();
    tags.dedup();
    tags
}

/// `filterBoardTickets`. The query matches when every word of it appears in the ticket's title,
/// description, ids or labels.
/// Returns the indices of the matching tickets.
pub(crate) fn filter_tickets(
    tickets: &[BoardTicket],
    query: &str,
    preferences: &KanbanViewPreferences,
    active_tag: &str,
) -> Vec<usize> {
    let words = query
        .split_whitespace()
        .map(str::to_lowercase)
        .collect::<Vec<_>>();
    tickets
        .iter()
        .enumerate()
        .filter(|(_, ticket)| {
            preferences.priority == "all"
                || priority_select_value(ticket.issue.priority) == preferences.priority
        })
        .filter(|(_, ticket)| match preferences.estimate.as_str() {
            "all" => true,
            "none" => estimate_to_tshirt(ticket.issue.estimate).is_none(),
            size => estimate_to_tshirt(ticket.issue.estimate) == Some(size),
        })
        .filter(|(_, ticket)| {
            active_tag == "all" || ticket.issue.labels.iter().any(|label| label == active_tag)
        })
        .filter(|(_, ticket)| {
            if words.is_empty() {
                return true;
            }
            let haystack = format!(
                "{}\n{}\n{}\n{}\n{}",
                ticket.issue.title,
                ticket.issue.description,
                ticket.issue.id,
                ticket.display_id,
                ticket.issue.labels.join(" ")
            )
            .to_lowercase();
            words.iter().all(|word| haystack.contains(word.as_str()))
        })
        .map(|(index, _)| index)
        .collect()
}

fn closed_time(ticket: &BoardTicket) -> Option<i64> {
    parse_time_ms(
        ticket
            .issue
            .closed_at
            .as_deref()
            .or(ticket.issue.updated_at.as_deref())
            .or(ticket.issue.created_at.as_deref()),
    )
}

fn updated_time(ticket: &BoardTicket) -> Option<i64> {
    parse_time_ms(
        ticket
            .issue
            .updated_at
            .as_deref()
            .or(ticket.issue.created_at.as_deref()),
    )
}

fn created_time(ticket: &BoardTicket) -> Option<i64> {
    parse_time_ms(ticket.issue.created_at.as_deref())
}

/// `compareBoardTicketTimes`: an unknown time stays at the bottom in both directions.
fn compare_times(left: Option<i64>, right: Option<i64>, ascending: bool) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    match (left, right) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
        (Some(left), Some(right)) if ascending => left.cmp(&right),
        (Some(left), Some(right)) => right.cmp(&left),
    }
}

/// `sortBoardTickets`: Done defaults to newest-closed first; the other lanes keep Beads' order
/// until a sort is chosen.
/// Sorts ticket indices (into `tickets`) in place.
pub(crate) fn sort_tickets(
    indices: &mut [usize],
    tickets: &[BoardTicket],
    sort: KanbanSort,
    column: &str,
) {
    let at = |index: &usize| &tickets[*index];
    match sort {
        KanbanSort::Default => {
            if column == "done" {
                indices.sort_by(|left, right| {
                    compare_times(closed_time(at(left)), closed_time(at(right)), false)
                });
            }
        }
        KanbanSort::UpdatedDesc | KanbanSort::UpdatedAsc => {
            let ascending = sort == KanbanSort::UpdatedAsc;
            indices.sort_by(|left, right| {
                compare_times(updated_time(at(left)), updated_time(at(right)), ascending)
            });
        }
        KanbanSort::CreatedDesc | KanbanSort::CreatedAsc => {
            let ascending = sort == KanbanSort::CreatedAsc;
            indices.sort_by(|left, right| {
                compare_times(created_time(at(left)), created_time(at(right)), ascending)
            });
        }
        KanbanSort::PriorityAsc | KanbanSort::PriorityDesc => {
            let ascending = sort == KanbanSort::PriorityAsc;
            let tier = |ticket: &BoardTicket| {
                priority_select_value(ticket.issue.priority)
                    .parse::<i64>()
                    .unwrap_or(3)
            };
            indices.sort_by(|left, right| {
                let (left, right) = (at(left), at(right));
                let delta = tier(left).cmp(&tier(right));
                let delta = if ascending { delta } else { delta.reverse() };
                delta
                    .then_with(|| compare_times(updated_time(left), updated_time(right), ascending))
            });
        }
    }
}
