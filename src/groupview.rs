//! What the group screens say (desktop/api/API.md, "Groups"): the sidebar
//! line, the chat header, "Delivered to 3 of 5" with its note, each
//! member's line in Details, and the ring under your message with one arc
//! per member. Pure and GTK-free; every number comes from the engine's
//! receipts (a member counts as delivered only after its signed receipt).

use crate::api::{Group, GroupMember, GroupMessage, GroupReceipt, GroupStatus, ReceiptState};
use crate::present::TickTone;
use crate::strings::{tr, trf};

/// The group's line in the sidebar: the newest message ("Ali: see you"),
/// else the member count.
pub fn sidebar_line(g: &Group) -> String {
    match (&g.last_from, &g.last_text) {
        (Some(from), Some(text)) if !from.is_empty() && !text.is_empty() => {
            trf("group.lastFrom", &[("name", from), ("text", text)])
        }
        _ => trf("group.members", &[("n", &g.members.to_string())]),
    }
}

/// The chat header's second line.
pub fn header_line(g: &Group) -> String {
    match g.status {
        GroupStatus::Left => tr("group.left").to_string(),
        GroupStatus::Removed => tr("group.removed").to_string(),
        _ => trf("group.subtitle", &[("n", &g.members.to_string())]),
    }
}

/// The banner over the composer when you can't write to the group any more.
pub fn closed_banner(g: &Group) -> Option<&'static str> {
    match g.status {
        GroupStatus::Left => Some(tr("group.left")),
        GroupStatus::Removed => Some(tr("group.removed")),
        _ => None,
    }
}

/// A member's state as the ring draws it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arc {
    /// A signed receipt came (successFill).
    Delivered,
    /// On this computer, or passed on and not confirmed (warningFill).
    Waiting,
    /// Left this computer, no receipt yet (borderStrong).
    Sent,
    /// Given up (dangerFill).
    Failed,
}

impl Arc {
    /// The colour token of the arc.
    pub fn color(self) -> &'static str {
        match self {
            Arc::Delivered => "successFill",
            Arc::Waiting => "warningFill",
            Arc::Sent => "borderStrong",
            Arc::Failed => "dangerFill",
        }
    }
}

fn arc_of(r: &GroupReceipt) -> Arc {
    match r.state {
        ReceiptState::Delivered => Arc::Delivered,
        ReceiptState::Sent => Arc::Sent,
        ReceiptState::Failed => Arc::Failed,
        ReceiptState::Waiting | ReceiptState::Unknown => Arc::Waiting,
    }
}

/// One arc per member, in the receipts' order.
pub fn ring(m: &GroupMessage) -> Vec<Arc> {
    m.receipts.iter().map(arc_of).collect()
}

/// Where each arc starts and how far it sweeps (radians, clockwise from
/// the top), with a small gap between arcs; one member is a full circle.
pub fn arc_geometry(n: usize) -> Vec<(f64, f64)> {
    let tau = std::f64::consts::TAU;
    if n == 0 {
        return Vec::new();
    }
    if n == 1 {
        return vec![(-tau / 4.0, tau)];
    }
    let seg = tau / n as f64;
    let gap = (0.23_f64).min(seg * 0.3);
    (0..n)
        .map(|i| (-tau / 4.0 + i as f64 * seg + gap / 2.0, seg - gap))
        .collect()
}

/// "Delivered to 3 of 5" with its tone and the note ("1 waiting · 1 not
/// confirmed yet"), from the receipts (the engine's own counts when there
/// are no receipts).
pub fn tick(m: &GroupMessage) -> (String, TickTone, Option<String>) {
    let (mut delivered, mut waiting, mut sent, mut failed) = (0u32, 0u32, 0u32, 0u32);
    for r in &m.receipts {
        match arc_of(r) {
            Arc::Delivered => delivered += 1,
            Arc::Waiting => waiting += 1,
            Arc::Sent => sent += 1,
            Arc::Failed => failed += 1,
        }
    }
    let total = if m.receipts.is_empty() {
        delivered = m.delivered;
        m.total
    } else {
        m.receipts.len() as u32
    };
    let text = trf(
        "group.delivered",
        &[("n", &delivered.to_string()), ("total", &total.to_string())],
    );
    let tone = if total > 0 && delivered == total {
        TickTone::Delivered
    } else if failed > 0 {
        TickTone::Problem
    } else {
        TickTone::Normal
    };
    let mut parts = Vec::new();
    if waiting > 0 {
        parts.push(trf("group.note.waiting", &[("n", &waiting.to_string())]));
    }
    if sent > 0 {
        parts.push(trf("group.note.sent", &[("n", &sent.to_string())]));
    }
    if failed > 0 {
        parts.push(trf("group.note.failed", &[("n", &failed.to_string())]));
    }
    let note = (!parts.is_empty()).then(|| parts.join(" \u{00B7} "));
    (text, tone, note)
}

/// One member's line in Details: state text and tone.
pub fn receipt_line(r: &GroupReceipt) -> (String, TickTone) {
    let (state, tone) = match r.state {
        ReceiptState::Delivered => (tr("tick.delivered"), TickTone::Delivered),
        ReceiptState::Sent => (tr("tick.sent"), TickTone::Normal),
        ReceiptState::Failed => (tr("tick.failed"), TickTone::Problem),
        ReceiptState::Waiting | ReceiptState::Unknown => {
            (tr("group.receipt.waiting"), TickTone::Waiting)
        }
    };
    let text = if r.via.as_deref() == Some("relay") {
        format!("{state} \u{00B7} {}", tr("group.receipt.viaMember"))
    } else {
        state.to_string()
    };
    (text, tone)
}

/// The tag after a member's name in the members list.
pub fn member_tags(m: &GroupMember) -> Vec<&'static str> {
    let mut tags = Vec::new();
    if m.admin {
        tags.push(tr("group.admin"));
    }
    if !m.friend && !m.me {
        tags.push(tr("group.notFriend"));
    }
    tags
}

/// A member's name as the chat shows it ("You" for you).
pub fn member_name(m: &GroupMember) -> String {
    if m.me {
        tr("you.title").to_string()
    } else {
        m.name.clone()
    }
}

/// The most members a group has, you included (owner decision 2026-10-28:
/// 16). Only its creator (its one admin) adds and removes; the admin can't
/// leave.
pub const MAX_MEMBERS: usize = 16;

#[cfg(test)]
mod tests {
    use super::*;

    fn r(state: ReceiptState, via: &str) -> GroupReceipt {
        GroupReceipt {
            address: "rvn1q".into(),
            name: "Ali".into(),
            state,
            via: Some(via.into()),
        }
    }

    fn msg(receipts: Vec<GroupReceipt>) -> GroupMessage {
        GroupMessage {
            id: "g1".into(),
            group: "ab".into(),
            from_address: "rvn1me".into(),
            from_name: "You".into(),
            mine: true,
            text: "hi".into(),
            at_ms: 1,
            via_name: None,
            delivered: 0,
            total: 0,
            receipts,
        }
    }

    #[test]
    fn delivered_to_n_of_total() {
        let m = msg(vec![
            r(ReceiptState::Delivered, "direct"),
            r(ReceiptState::Delivered, "relay"),
            r(ReceiptState::Delivered, "direct"),
            r(ReceiptState::Waiting, "direct"),
            r(ReceiptState::Sent, "relay"),
        ]);
        let (text, tone, note) = tick(&m);
        assert_eq!(text, "Delivered to 3 of 5");
        assert_eq!(tone, TickTone::Normal);
        assert_eq!(
            note.as_deref(),
            Some("1 waiting \u{00B7} 1 not confirmed yet")
        );
        assert_eq!(
            ring(&m),
            vec![
                Arc::Delivered,
                Arc::Delivered,
                Arc::Delivered,
                Arc::Waiting,
                Arc::Sent
            ]
        );
        let all = msg(vec![r(ReceiptState::Delivered, "direct")]);
        assert_eq!(tick(&all).1, TickTone::Delivered);
        assert_eq!(tick(&all).2, None);
        let failed = msg(vec![r(ReceiptState::Failed, "direct")]);
        assert_eq!(tick(&failed).1, TickTone::Problem);
        // No receipts listed: the engine's own counts.
        let mut bare = msg(Vec::new());
        bare.delivered = 2;
        bare.total = 4;
        assert_eq!(tick(&bare).0, "Delivered to 2 of 4");
    }

    #[test]
    fn receipt_lines() {
        assert_eq!(
            receipt_line(&r(ReceiptState::Delivered, "direct")),
            ("Delivered".into(), TickTone::Delivered)
        );
        let (relay, _) = receipt_line(&r(ReceiptState::Delivered, "relay"));
        assert!(relay.starts_with("Delivered \u{00B7} "), "{relay}");
        assert_eq!(
            receipt_line(&r(ReceiptState::Waiting, "direct")).1,
            TickTone::Waiting
        );
        assert_eq!(
            receipt_line(&r(ReceiptState::Failed, "direct")).1,
            TickTone::Problem
        );
    }

    #[test]
    fn arcs_share_the_circle() {
        assert!(arc_geometry(0).is_empty());
        assert_eq!(arc_geometry(1).len(), 1);
        let five = arc_geometry(5);
        assert_eq!(five.len(), 5);
        let total: f64 = five.iter().map(|(_, s)| s).sum();
        assert!(total < std::f64::consts::TAU && total > 5.0);
        // In order, without overlapping.
        for w in five.windows(2) {
            assert!(w[0].0 + w[0].1 < w[1].0);
        }
        assert_eq!(Arc::Delivered.color(), "successFill");
    }

    #[test]
    fn group_lines() {
        let mut g = Group {
            id: "ab".into(),
            name: "Trip".into(),
            short_id: String::new(),
            status: GroupStatus::Active,
            admin: false,
            admin_address: String::new(),
            admin_name: String::new(),
            members: 6,
            unread: 0,
            last_ms: None,
            last_text: None,
            last_from: None,
        };
        assert_eq!(sidebar_line(&g), "6 members");
        g.last_from = Some("Ali".into());
        g.last_text = Some("see you".into());
        assert_eq!(sidebar_line(&g), "Ali: see you");
        assert!(header_line(&g).starts_with("6 members"));
        assert_eq!(closed_banner(&g), None);
        g.status = GroupStatus::Left;
        assert!(closed_banner(&g).is_some());
        let m = GroupMember {
            address: "a".into(),
            short_address: String::new(),
            name: "Sara".into(),
            label: "Sara".into(),
            friend: false,
            admin: true,
            me: false,
        };
        assert_eq!(member_tags(&m).len(), 2);
        assert_eq!(member_name(&m), "Sara");
    }
}
