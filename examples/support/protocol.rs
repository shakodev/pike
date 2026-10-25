use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Marker {
    Ready(u64, u32),
    Size(u64, u16, u16),
    Final(u64),
}

impl Marker {
    pub fn parse(title: &str) -> Option<Self> {
        let mut fields = title.split(':');
        if fields.next()? != "pike" {
            return None;
        }
        let kind = fields.next()?;
        let id = fields.next()?.parse().ok()?;
        if !(1..=2).contains(&id) {
            return None;
        }
        let marker = match kind {
            "ready" => {
                let pid = fields.next()?.parse().ok()?;
                if pid == 0 {
                    return None;
                }
                Self::Ready(id, pid)
            }
            "size" => {
                let cols = fields.next()?.parse().ok()?;
                let rows = fields.next()?.parse().ok()?;
                if !(1..=512).contains(&cols) || !(1..=1000).contains(&rows) {
                    return None;
                }
                Self::Size(id, cols, rows)
            }
            "final" => Self::Final(id),
            _ => return None,
        };
        fields.next().is_none().then_some(marker)
    }

    fn id(self) -> u64 {
        match self {
            Self::Ready(id, _) | Self::Size(id, _, _) | Self::Final(id) => id,
        }
    }
}

#[derive(Default)]
pub struct ProbeState {
    pub pid: Option<u32>,
    pub first_size: Option<(u16, u16)>,
    pub latest_size: Option<(u16, u16)>,
    pub final_seen: bool,
    pub exit_code: Option<i32>,
    pub wakeups: u64,
    pub resizes: u64,
}

pub fn route_marker(
    states: &mut BTreeMap<u64, ProbeState>,
    origin: u64,
    title: &str,
) -> Option<Marker> {
    let marker = Marker::parse(title)?;
    if marker.id() != origin {
        return None;
    }
    let state = states.get_mut(&origin)?;
    match marker {
        Marker::Ready(_, pid) => state.pid = Some(pid),
        Marker::Size(_, cols, rows) => {
            state.first_size.get_or_insert((cols, rows));
            state.latest_size = Some((cols, rows));
        }
        Marker::Final(_) => state.final_seen = true,
    }
    Some(marker)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn background_markers_update_the_origin_not_the_focused_session() {
        let mut states = BTreeMap::from([(1, ProbeState::default()), (2, ProbeState::default())]);
        assert_eq!(
            route_marker(&mut states, 2, "pike:ready:2:123"),
            Some(Marker::Ready(2, 123))
        );
        assert!(states[&1].pid.is_none());
        assert_eq!(states[&2].pid, Some(123));
        assert!(route_marker(&mut states, 1, "pike:final:2").is_none());
        assert!(!states[&2].final_seen);
        assert!(route_marker(&mut states, 9, "pike:final:1").is_none());
    }

    #[test]
    fn dimensions_keep_first_and_latest_without_an_unbounded_history() {
        let mut states = BTreeMap::from([(1, ProbeState::default())]);
        route_marker(&mut states, 1, "pike:size:1:80:24");
        route_marker(&mut states, 1, "pike:size:1:100:30");
        assert_eq!(states[&1].first_size, Some((80, 24)));
        assert_eq!(states[&1].latest_size, Some((100, 30)));
    }

    #[test]
    fn arbitrary_titles_and_malformed_markers_are_rejected() {
        for title in [
            "private shell title",
            "pike:ready:1:0",
            "pike:final:3",
            "pike:final:1:extra",
            "pike:size:1:0:24",
            "pike:size:1:513:24",
            "pike:size:1:80:1001",
        ] {
            assert_eq!(Marker::parse(title), None);
        }
    }
}
