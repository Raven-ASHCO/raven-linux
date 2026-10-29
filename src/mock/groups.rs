//! Groups in the mock (debug builds only), following desktop/api/API.md
//! ("Groups"; the mock answers as a lab build with groups on): one demo group,
//! "Weekend trip", whose admin is the demo friend Ali. Two of its members
//! (Sara, Noor) are not your friends. Its demo events: a member's message,
//! receipts that fill your message's ring one member at a time, and Mina's
//! copy delivered when she comes back.

use serde_json::{json, Value};

use super::{arg_str, bad_request, to_value, Due, State, ALI, MAX_TEXT, ME_ADDRESS, MINA, RENA};
use crate::api::*;
use crate::groupview::MAX_MEMBERS;
use crate::strings::tr;

/// The demo group's id (32 hex characters, as the engine's).
pub(super) const TRIP: &str = "7f2a91c40b3e4d5a8c6e1f0b9d2a3c4e";
const SARA: &str = "rvn1qsara0demo0member0not0a0friend0x";
const NOOR: &str = "rvn1qnoor0demo0member0not0a0friend0y";

pub(super) struct MockGroup {
    pub group: Group,
    pub members: Vec<GroupMember>,
    pub messages: Vec<GroupMessage>,
    /// Id of the newest message read on this computer.
    pub read_up_to: Option<String>,
}

#[derive(Default)]
pub(super) struct MockGroups {
    pub list: Vec<MockGroup>,
    next: u64,
}

fn member(address: &str, name: &str, friend: bool, admin: bool, me: bool) -> GroupMember {
    GroupMember {
        address: address.into(),
        short_address: super::short(address),
        name: name.into(),
        label: name.into(),
        friend,
        admin,
        me,
    }
}

fn receipt(address: &str, name: &str, state: ReceiptState, via: &str) -> GroupReceipt {
    GroupReceipt {
        address: address.into(),
        name: name.into(),
        state,
        via: Some(via.into()),
    }
}

fn count(m: &mut GroupMessage) {
    m.total = m.receipts.len() as u32;
    m.delivered = m
        .receipts
        .iter()
        .filter(|r| r.state == ReceiptState::Delivered)
        .count() as u32;
}

impl MockGroups {
    pub fn seed(now: u64) -> Self {
        let day = 24 * 3_600_000;
        let msg = |id: &str, from: &str, name: &str, text: &str, at_ms: u64| GroupMessage {
            id: id.into(),
            group: TRIP.into(),
            from_address: from.into(),
            from_name: name.into(),
            mine: from == ME_ADDRESS,
            text: text.into(),
            at_ms,
            via_name: None,
            delivered: 0,
            total: 0,
            receipts: Vec::new(),
        };
        let mut mine = msg(
            "g4",
            ME_ADDRESS,
            "You",
            "Great, I'll bring the map and snacks.",
            now - 25 * 60_000,
        );
        mine.receipts = vec![
            receipt(ALI, "Ali", ReceiptState::Delivered, "direct"),
            receipt(MINA, "Mina", ReceiptState::Waiting, "direct"),
            receipt(RENA, "Rena", ReceiptState::Delivered, "direct"),
            receipt(SARA, "Sara", ReceiptState::Delivered, "relay"),
            receipt(NOOR, "Noor", ReceiptState::Sent, "relay"),
        ];
        count(&mut mine);
        let mut sara = msg(
            "g2",
            SARA,
            "Sara",
            "Me! I'll bring the camping stove.",
            now - day - 40 * 60_000,
        );
        sara.via_name = Some("Ali".into());
        let messages = vec![
            msg(
                "g1",
                ALI,
                "Ali",
                "Who's in for the cabin this weekend?",
                now - day - 50 * 60_000,
            ),
            sara,
            msg(
                "g3",
                MINA,
                "Mina",
                "Booked, Friday to Sunday.",
                now - 40 * 60_000,
            ),
            mine,
        ];
        let group = Group {
            id: TRIP.into(),
            name: "Weekend trip".into(),
            short_id: "7f2a91c4".into(),
            status: GroupStatus::Active,
            admin: false,
            admin_address: ALI.into(),
            admin_name: "Ali".into(),
            members: 6,
            unread: 0,
            last_ms: None,
            last_text: None,
            last_from: None,
        };
        let mut g = MockGroup {
            group,
            members: vec![
                member(ALI, "Ali", true, true, false),
                member(ME_ADDRESS, "You", false, false, true),
                member(MINA, "Mina", true, false, false),
                member(RENA, "Rena", true, false, false),
                member(SARA, "Sara", false, false, false),
                member(NOOR, "Noor", false, false, false),
            ],
            messages,
            read_up_to: Some("g4".into()),
        };
        g.refresh();
        Self {
            list: vec![g],
            next: 100,
        }
    }

    pub fn find(&self, id: &str) -> Result<&MockGroup, ApiError> {
        self.list
            .iter()
            .find(|g| g.group.id == id)
            .ok_or_else(not_found)
    }

    pub fn find_mut(&mut self, id: &str) -> Result<&mut MockGroup, ApiError> {
        self.list
            .iter_mut()
            .find(|g| g.group.id == id)
            .ok_or_else(not_found)
    }

    fn new_id(&mut self) -> String {
        self.next += 1;
        format!("g{}", self.next)
    }
}

fn not_admin() -> ApiError {
    ApiError::new(
        codes::GROUP_NOT_ADMIN,
        "Only the group's admin adds and removes members.",
        "mock: not the admin",
    )
}

fn not_found() -> ApiError {
    ApiError::new(
        codes::GROUP_NOT_FOUND,
        tr("error.badRequest"),
        "mock: no such group",
    )
}

impl MockGroup {
    /// The list fields that follow from the messages.
    fn refresh(&mut self) {
        let start = self
            .read_up_to
            .as_deref()
            .and_then(|id| self.messages.iter().rposition(|m| m.id == id))
            .map_or(0, |i| i + 1);
        self.group.unread = self.messages[start..].iter().filter(|m| !m.mine).count() as u32;
        if let Some(last) = self.messages.last() {
            self.group.last_ms = Some(last.at_ms);
            self.group.last_text = Some(last.text.clone());
            self.group.last_from = Some(last.from_name.clone());
        }
    }
}

impl State {
    /// `groups.*` ops; `None` for any other op.
    pub(super) fn group_op(
        &mut self,
        op: &str,
        args: &Value,
        now: u64,
    ) -> Option<Result<Value, ApiError>> {
        let r = match op {
            "groups.list" => {
                let groups: Vec<Group> = self.groups.list.iter().map(|g| g.group.clone()).collect();
                Ok(json!({ "available": true, "groups": groups }))
            }
            "groups.get" => self.require_ready().and_then(|_| {
                let g = self.groups.find(arg_str(args, "group")?)?;
                Ok(json!({ "group": g.group, "members": g.members }))
            }),
            "groups.history" => self.require_ready().and_then(|_| {
                let g = self.groups.find(arg_str(args, "group")?)?;
                let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(100) as usize;
                let end = match args.get("before").and_then(Value::as_str) {
                    Some(b) => g.messages.iter().position(|m| m.id == b).unwrap_or(0),
                    None => g.messages.len(),
                };
                let start = end.saturating_sub(limit.clamp(1, 500));
                to_value(&json!({ "messages": g.messages[start..end] }))
            }),
            "groups.send" => self
                .require_ready()
                .and_then(|_| self.group_send(args, now)),
            "groups.create" => self
                .require_ready()
                .and_then(|_| self.group_create(args, now)),
            "groups.add" => self.require_ready().and_then(|_| self.group_add(args, now)),
            "groups.remove" => self.require_ready().and_then(|_| {
                let id = arg_str(args, "group")?.to_string();
                let member = arg_str(args, "member")?.to_string();
                let g = self.groups.find_mut(&id)?;
                if !g.group.admin {
                    return Err(not_admin());
                }
                let before = g.members.len();
                g.members.retain(|m| m.address != member || m.me);
                if g.members.len() == before {
                    return Err(bad_request(format!("{member} is not in this group")));
                }
                g.group.members = g.members.len() as u32;
                let out = json!({ "group": g.group, "members": g.members });
                self.emit(now, EventKind::GroupChanged { group: id });
                Ok(out)
            }),
            "groups.leave" => self.require_ready().and_then(|_| {
                let id = arg_str(args, "group")?.to_string();
                let g = self.groups.find_mut(&id)?;
                if g.group.admin {
                    return Err(ApiError::new(
                        codes::GROUP_ADMIN_CANNOT_LEAVE,
                        "You made this group, so you can't leave it. Remove the others instead.",
                        "mock: the admin can't leave",
                    ));
                }
                g.group.status = GroupStatus::Left;
                let out = json!({ "group": g.group });
                self.emit(now, EventKind::GroupChanged { group: id });
                Ok(out)
            }),
            "groups.mark_read" => self.require_ready().and_then(|_| {
                let id = arg_str(args, "group")?.to_string();
                let g = self.groups.find_mut(&id)?;
                g.read_up_to = g.messages.last().map(|m| m.id.clone());
                let changed = g.group.unread != 0;
                g.refresh();
                if changed {
                    self.emit(now, EventKind::GroupChanged { group: id.clone() });
                }
                Ok(json!({ "group": id, "unread": 0 }))
            }),
            _ => return None,
        };
        Some(r)
    }

    /// Friends named in `members`, as group members.
    fn friends_named(&self, args: &Value) -> Result<Vec<GroupMember>, ApiError> {
        let Some(Value::Array(items)) = args.get("members") else {
            return Err(bad_request("\"members\" must be a list of addresses"));
        };
        let mut out: Vec<GroupMember> = Vec::new();
        for item in items {
            let address = item
                .as_str()
                .ok_or_else(|| bad_request("\"members\" must be a list of addresses"))?;
            let c = self
                .contacts
                .iter()
                .find(|c| c.c.address == address)
                .ok_or_else(super::not_a_contact)?;
            if !out.iter().any(|m| m.address == address) {
                out.push(member(address, &c.c.name, true, false, false));
            }
        }
        Ok(out)
    }

    fn group_create(&mut self, args: &Value, now: u64) -> Result<Value, ApiError> {
        let name: String = arg_str(args, "name")?.trim().chars().take(64).collect();
        if name.is_empty() {
            return Err(ApiError::new(
                "GROUP_BAD_NAME",
                "Give the group a name.",
                "mock: empty name",
            ));
        }
        let mut members = vec![member(ME_ADDRESS, "You", false, true, true)];
        members.extend(self.friends_named(args)?);
        if members.len() < 2 {
            return Err(bad_request("\"members\" names no friend"));
        }
        if members.len() > MAX_MEMBERS {
            return Err(ApiError::new(
                codes::GROUP_TOO_LARGE,
                "A group has at most 16 members, you included.",
                "mock: too many members",
            ));
        }
        let n = self.groups.new_id();
        let id = format!("{:0>32}", n.trim_start_matches('g'));
        let group = Group {
            id: id.clone(),
            name,
            short_id: id[..8].into(),
            status: GroupStatus::Active,
            admin: true,
            admin_address: ME_ADDRESS.into(),
            admin_name: "You".into(),
            members: members.len() as u32,
            unread: 0,
            last_ms: None,
            last_text: None,
            last_from: None,
        };
        let out = json!({ "group": group, "members": members });
        self.groups.list.push(MockGroup {
            group,
            members,
            messages: Vec::new(),
            read_up_to: None,
        });
        self.emit(now, EventKind::GroupChanged { group: id });
        Ok(out)
    }

    fn group_add(&mut self, args: &Value, now: u64) -> Result<Value, ApiError> {
        let id = arg_str(args, "group")?.to_string();
        let new = self.friends_named(args)?;
        let g = self.groups.find_mut(&id)?;
        if !g.group.admin {
            return Err(not_admin());
        }
        for m in new {
            if !g.members.iter().any(|x| x.address == m.address) {
                g.members.push(m);
            }
        }
        if g.members.len() > MAX_MEMBERS {
            return Err(ApiError::new(
                codes::GROUP_TOO_LARGE,
                "A group has at most 16 members, you included.",
                "mock: too many members",
            ));
        }
        g.group.members = g.members.len() as u32;
        let out = json!({ "group": g.group, "members": g.members });
        self.emit(now, EventKind::GroupChanged { group: id });
        Ok(out)
    }

    fn group_send(&mut self, args: &Value, now: u64) -> Result<Value, ApiError> {
        let id = arg_str(args, "group")?.to_string();
        if self.groups.find(&id)?.group.status != GroupStatus::Active {
            return Err(ApiError::new(
                codes::GROUP_NOT_MEMBER,
                "You are no longer in this group, so you can't write to it.",
                "mock: not a member",
            ));
        }
        let text = arg_str(args, "text")?.to_string();
        if text.trim().is_empty() {
            return Err(bad_request("empty message"));
        }
        if text.len() > MAX_TEXT {
            return Err(ApiError::new(
                codes::MESSAGE_TOO_LONG,
                tr("error.messageTooLong"),
                format!("mock: {} bytes", text.len()),
            ));
        }
        let presence_of = |s: &State, address: &str| {
            s.contacts
                .iter()
                .find(|c| c.c.address == address)
                .map(|c| (c.c.presence, c.c.verified))
        };
        let members = self.groups.find(&id)?.members.clone();
        let msg_id = self.groups.new_id();
        let mut receipts = Vec::new();
        let mut plan = Vec::new();
        for (i, m) in members.iter().filter(|m| !m.me).enumerate() {
            let (state, via) = match presence_of(self, &m.address) {
                Some((Presence::InTouch, _)) => (ReceiptState::Sent, "direct"),
                Some(_) => (ReceiptState::Waiting, "direct"),
                None => (ReceiptState::Waiting, "relay"),
            };
            receipts.push(receipt(&m.address, &m.name, state, via));
            // Friends in touch confirm in about a second; members who aren't
            // your friends confirm through the member who passed it on.
            if state == ReceiptState::Sent || via == "relay" {
                plan.push((now + 900 + 450 * i as u64, m.address.clone()));
            }
        }
        let mut message = GroupMessage {
            id: msg_id.clone(),
            group: id.clone(),
            from_address: ME_ADDRESS.into(),
            from_name: "You".into(),
            mine: true,
            text,
            at_ms: now,
            via_name: None,
            delivered: 0,
            total: 0,
            receipts,
        };
        count(&mut message);
        let g = self.groups.find_mut(&id)?;
        g.messages.push(message.clone());
        g.read_up_to = Some(msg_id.clone());
        g.refresh();
        for (at, member) in plan {
            self.schedule(
                at,
                Due::GroupReceipt {
                    group: id.clone(),
                    id: msg_id.clone(),
                    member,
                },
            );
        }
        Ok(json!({ "message": message }))
    }

    /// A member's copy of your message was confirmed.
    pub(super) fn group_receipt(&mut self, at: u64, group: &str, id: &str, member: &str) {
        let Ok(g) = self.groups.find_mut(group) else {
            return;
        };
        let Some(m) = g.messages.iter_mut().find(|m| m.id == id) else {
            return;
        };
        let Some(r) = m.receipts.iter_mut().find(|r| r.address == member) else {
            return;
        };
        if r.state == ReceiptState::Delivered {
            return;
        }
        r.state = ReceiptState::Delivered;
        count(m);
        self.emit(
            at,
            EventKind::GroupChanged {
                group: group.into(),
            },
        );
    }

    /// A member's message came.
    pub(super) fn group_incoming(&mut self, at: u64, group: &str, from: &str, text: &str) {
        let id = self.groups.new_id();
        let Ok(g) = self.groups.find_mut(group) else {
            return;
        };
        let name = g
            .members
            .iter()
            .find(|m| m.address == from)
            .map(|m| m.name.clone())
            .unwrap_or_default();
        let message = GroupMessage {
            id,
            group: group.into(),
            from_address: from.into(),
            from_name: name,
            mine: false,
            text: text.into(),
            at_ms: at,
            via_name: (from != ALI).then(|| "Ali".to_string()),
            delivered: 0,
            total: 0,
            receipts: Vec::new(),
        };
        g.messages.push(message.clone());
        g.refresh();
        self.emit(
            at,
            EventKind::GroupMessage {
                group: group.into(),
                message,
            },
        );
    }

    /// The demo's group events, after the first poll.
    pub(super) fn start_group_demo(&mut self, now: u64) {
        self.schedule(
            now + 4_000,
            Due::GroupIncoming {
                group: TRIP.into(),
                from: NOOR.into(),
                text: "Leaving from Ali's place at 9 on Saturday.".into(),
            },
        );
        self.schedule(
            now + 7_500,
            Due::GroupReceipt {
                group: TRIP.into(),
                id: "g4".into(),
                member: NOOR.into(),
            },
        );
        // Mina comes back with the 1:1 demo; her copy is delivered then.
        self.schedule(
            now + 10_800,
            Due::GroupReceipt {
                group: TRIP.into(),
                id: "g4".into(),
                member: MINA.into(),
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Clock, MockRaven};
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;

    fn ready() -> (MockRaven, Arc<AtomicU64>) {
        let t = Arc::new(AtomicU64::new(1_792_505_100_000));
        (MockRaven::with_clock(Clock::Manual(t.clone()), true), t)
    }

    #[test]
    fn the_demo_group() {
        let (m, _) = ready();
        let list = m.groups_list().unwrap();
        assert!(list.available);
        let g = &list.groups[0];
        assert_eq!(
            (g.name.as_str(), g.members, g.unread),
            ("Weekend trip", 6, 0)
        );
        assert_eq!(g.last_from.as_deref(), Some("You"));
        let d = m.groups_get(TRIP).unwrap();
        assert_eq!(d.members.len(), 6);
        assert_eq!(d.members.iter().filter(|m| !m.friend && !m.me).count(), 2);
        assert!(d.members.iter().any(|m| m.admin && m.name == "Ali"));
        let h = m.groups_history(TRIP, None, 100).unwrap();
        assert_eq!(h.len(), 4);
        let mine = h.last().unwrap();
        assert!(mine.mine);
        assert_eq!((mine.delivered, mine.total), (3, 5));
        assert_eq!(h[1].via_name.as_deref(), Some("Ali"));
        assert_eq!(m.groups_history(TRIP, Some("g3"), 1).unwrap()[0].id, "g2");
        assert!(m.groups_get("nope").unwrap_err().is(codes::GROUP_NOT_FOUND));
    }

    #[test]
    fn sending_fills_the_ring_one_member_at_a_time() {
        let (m, t) = ready();
        let after = m.events_poll(0).unwrap().seq;
        let (sent, note) = m.groups_send(TRIP, "Who has the key?").unwrap();
        assert_eq!(note, None);
        assert!(sent.mine);
        assert_eq!(sent.total, 5);
        assert_eq!(sent.delivered, 0, "nothing is delivered before a receipt");
        // Ali is in touch: his copy left; the others wait.
        let ali = sent.receipts.iter().find(|r| r.name == "Ali").unwrap();
        assert_eq!(ali.state, ReceiptState::Sent);
        t.fetch_add(5_000, Ordering::SeqCst);
        let p = m.events_poll(after).unwrap();
        let changed = p
            .events
            .iter()
            .filter(|e| matches!(e.kind, EventKind::GroupChanged { .. }))
            .count();
        assert!(changed >= 2, "a change per confirmation: {changed}");
        let now = m.groups_history(TRIP, None, 100).unwrap();
        let mine = now.iter().find(|x| x.id == sent.id).unwrap();
        assert!(mine.delivered >= 2 && mine.delivered <= mine.total);
        assert_eq!(
            mine.receipts
                .iter()
                .find(|r| r.name == "Mina")
                .unwrap()
                .state,
            ReceiptState::Waiting,
            "Mina is away: hers waits"
        );
        assert!(m.groups_send(TRIP, "  ").is_err());
    }

    #[test]
    fn a_member_writes_and_it_counts_as_unread() {
        let (m, t) = ready();
        let after = m.events_poll(0).unwrap().seq; // starts the demo
        t.fetch_add(4_500, Ordering::SeqCst);
        let p = m.events_poll(after).unwrap();
        let msg = p
            .events
            .iter()
            .find_map(|e| match &e.kind {
                EventKind::GroupMessage { message, .. } => Some(message.clone()),
                _ => None,
            })
            .expect("Noor's message");
        assert_eq!(msg.from_name, "Noor");
        assert_eq!(msg.via_name.as_deref(), Some("Ali"));
        assert_eq!(m.groups_list().unwrap().groups[0].unread, 1);
        m.groups_mark_read(TRIP).unwrap();
        assert_eq!(m.groups_list().unwrap().groups[0].unread, 0);
    }

    #[test]
    fn create_add_remove_leave() {
        let (m, _) = ready();
        let friends: Vec<String> = m
            .contacts_list()
            .unwrap()
            .into_iter()
            .map(|c| c.address)
            .collect();
        let d = m.groups_create("Book club", &friends[..1]).unwrap();
        assert!(d.group.admin);
        assert_eq!(d.members.len(), 2);
        let id = d.group.id.clone();
        assert_eq!(id.len(), 32);
        let d = m.groups_add(&id, &friends[1..]).unwrap();
        assert_eq!(d.group.members, 4);
        let d = m.groups_remove(&id, &friends[2]).unwrap();
        assert_eq!(d.members.len(), 3);
        assert!(m
            .groups_leave(&id)
            .unwrap_err()
            .is(codes::GROUP_ADMIN_CANNOT_LEAVE));
        assert!(m
            .groups_create("x", &["rvn1qnotafriend".into()])
            .unwrap_err()
            .is(codes::NOT_A_CONTACT));
        // The demo group: you aren't its admin.
        assert!(m
            .groups_add(TRIP, &friends[..1])
            .unwrap_err()
            .is(codes::GROUP_NOT_ADMIN));
        let left = m.groups_leave(TRIP).unwrap();
        assert_eq!(left.status, GroupStatus::Left);
        assert!(m
            .groups_send(TRIP, "hi")
            .unwrap_err()
            .is(codes::GROUP_NOT_MEMBER));
        assert_eq!(
            m.groups_history(TRIP, None, 100).unwrap().len(),
            4,
            "the history stays"
        );
    }
}
