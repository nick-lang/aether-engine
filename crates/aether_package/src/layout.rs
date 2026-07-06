//! Room adjacency and in-room placement (shared by runtime tooling and generation).

use crate::RoomBoundsDef;

/// Default height for a room attached north/south of a parent.
pub const DEFAULT_NS_ROOM_HEIGHT: f32 = 100.0;
/// Default width for a room attached east/west of a parent.
pub const DEFAULT_EW_ROOM_WIDTH: f32 = 120.0;
/// Inset from a room edge when placing props along that edge.
pub const EDGE_INSET: f32 = 32.0;

const SNAP_EPS: f32 = 1.0;

/// Default main room for `minimal_explorer` and generation fallbacks.
pub fn default_main_room_bounds() -> RoomBoundsDef {
    RoomBoundsDef {
        x: -220.0,
        y: -140.0,
        width: 440.0,
        height: 280.0,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    North,
    South,
    East,
    West,
}

impl Direction {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "north" | "n" => Some(Self::North),
            "south" | "s" => Some(Self::South),
            "east" | "e" => Some(Self::East),
            "west" | "w" => Some(Self::West),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoomAnchor {
    Center,
    North,
    South,
    East,
    West,
}

impl RoomAnchor {
    pub fn for_direction(dir: Direction) -> Self {
        match dir {
            Direction::North => Self::North,
            Direction::South => Self::South,
            Direction::East => Self::East,
            Direction::West => Self::West,
        }
    }
}

impl RoomBoundsDef {
    pub fn min_x(&self) -> f32 {
        self.x
    }

    pub fn max_x(&self) -> f32 {
        self.x + self.width
    }

    pub fn min_y(&self) -> f32 {
        self.y
    }

    pub fn max_y(&self) -> f32 {
        self.y + self.height
    }

    pub fn area(&self) -> f32 {
        self.width * self.height
    }
}

/// Place a new room flush against `parent` on `direction`.
///
/// North/south children share the parent's width and x. East/west children share height and y.
pub fn attach_room(
    parent: &RoomBoundsDef,
    direction: Direction,
    width: f32,
    height: f32,
) -> RoomBoundsDef {
    match direction {
        Direction::North => RoomBoundsDef {
            x: parent.x,
            y: parent.max_y(),
            width: parent.width,
            height,
        },
        Direction::South => RoomBoundsDef {
            x: parent.x,
            y: parent.min_y() - height,
            width: parent.width,
            height,
        },
        Direction::East => RoomBoundsDef {
            x: parent.max_x(),
            y: parent.y,
            width,
            height: parent.height,
        },
        Direction::West => RoomBoundsDef {
            x: parent.min_x() - width,
            y: parent.y,
            width,
            height: parent.height,
        },
    }
}

pub fn default_attach_width(direction: Direction) -> f32 {
    match direction {
        Direction::East | Direction::West => DEFAULT_EW_ROOM_WIDTH,
        Direction::North | Direction::South => 0.0, // uses parent width
    }
}

pub fn default_attach_height(direction: Direction) -> f32 {
    match direction {
        Direction::North | Direction::South => DEFAULT_NS_ROOM_HEIGHT,
        Direction::East | Direction::West => 0.0, // uses parent height
    }
}

/// World position for a prop/player inside `room`.
pub fn place_in_room(room: &RoomBoundsDef, anchor: RoomAnchor) -> (f32, f32) {
    let (cx, cy) = room.center();
    match anchor {
        RoomAnchor::Center => (cx, cy),
        RoomAnchor::North => (cx, room.max_y() - EDGE_INSET),
        RoomAnchor::South => (cx, room.min_y() + EDGE_INSET),
        RoomAnchor::East => (room.max_x() - EDGE_INSET, cy),
        RoomAnchor::West => (room.min_x() + EDGE_INSET, cy),
    }
}

/// Pick the room farthest from `anchor` along `direction` (for chaining new attachments).
pub fn extremal_room_bounds(
    existing: &[RoomBoundsDef],
    anchor: &RoomBoundsDef,
    direction: Direction,
) -> RoomBoundsDef {
    let mut best = anchor.clone();
    for room in existing {
        if !shares_corridor(anchor, room, direction) {
            continue;
        }
        if is_farther_in_direction(room, &best, direction) {
            best = room.clone();
        }
    }
    best
}

fn shares_corridor(anchor: &RoomBoundsDef, room: &RoomBoundsDef, direction: Direction) -> bool {
    match direction {
        Direction::North | Direction::South => overlaps_x(anchor, room),
        Direction::East | Direction::West => overlaps_y(anchor, room),
    }
}

/// True when `candidate` lies strictly beyond `anchor` along `direction`.
pub fn room_is_beyond(
    candidate: &RoomBoundsDef,
    anchor: &RoomBoundsDef,
    direction: Direction,
) -> bool {
    is_farther_in_direction(candidate, anchor, direction)
}

fn is_farther_in_direction(
    candidate: &RoomBoundsDef,
    current: &RoomBoundsDef,
    direction: Direction,
) -> bool {
    match direction {
        Direction::North => candidate.max_y() > current.max_y() + SNAP_EPS,
        Direction::South => candidate.min_y() < current.min_y() - SNAP_EPS,
        Direction::East => candidate.max_x() > current.max_x() + SNAP_EPS,
        Direction::West => candidate.min_x() < current.min_x() - SNAP_EPS,
    }
}

/// Whether `b` is attached to `a` on `direction` (from `a`'s perspective).
pub fn rooms_share_edge(a: &RoomBoundsDef, b: &RoomBoundsDef, direction: Direction) -> bool {
    match direction {
        Direction::North => {
            (b.min_y() - a.max_y()).abs() <= SNAP_EPS && overlaps_x(a, b)
        }
        Direction::South => {
            (a.min_y() - b.max_y()).abs() <= SNAP_EPS && overlaps_x(a, b)
        }
        Direction::East => {
            (b.min_x() - a.max_x()).abs() <= SNAP_EPS && overlaps_y(a, b)
        }
        Direction::West => {
            (a.min_x() - b.max_x()).abs() <= SNAP_EPS && overlaps_y(a, b)
        }
    }
}

fn overlaps_x(a: &RoomBoundsDef, b: &RoomBoundsDef) -> bool {
    a.max_x() > b.min_x() + SNAP_EPS && b.max_x() > a.min_x() + SNAP_EPS
}

fn overlaps_y(a: &RoomBoundsDef, b: &RoomBoundsDef) -> bool {
    a.max_y() > b.min_y() + SNAP_EPS && b.max_y() > a.min_y() + SNAP_EPS
}

#[cfg(test)]
mod tests {
    use super::*;

    fn starting_grove() -> RoomBoundsDef {
        super::default_main_room_bounds()
    }

    #[test]
    fn north_room_aligns_with_parent() {
        let parent = starting_grove();
        let north = attach_room(
            &parent,
            Direction::North,
            DEFAULT_EW_ROOM_WIDTH,
            DEFAULT_NS_ROOM_HEIGHT,
        );
        assert_eq!(north.x, parent.x);
        assert_eq!(north.width, parent.width);
        assert!((north.min_y() - parent.max_y()).abs() < SNAP_EPS);
        assert!(rooms_share_edge(&parent, &north, Direction::North));
    }

    #[test]
    fn chains_second_north_room() {
        let parent = starting_grove();
        let north1 = attach_room(
            &parent,
            Direction::North,
            DEFAULT_EW_ROOM_WIDTH,
            DEFAULT_NS_ROOM_HEIGHT,
        );
        let existing = vec![parent.clone(), north1.clone()];
        let chain_parent = extremal_room_bounds(&existing, &parent, Direction::North);
        assert!((chain_parent.max_y() - north1.max_y()).abs() < SNAP_EPS);
        let north2 = attach_room(
            &chain_parent,
            Direction::North,
            DEFAULT_EW_ROOM_WIDTH,
            DEFAULT_NS_ROOM_HEIGHT,
        );
        assert!((north2.min_y() - north1.max_y()).abs() < SNAP_EPS);
        assert!(north2.max_y() > north1.max_y());
    }

    #[test]
    fn east_room_aligns_with_parent() {
        let parent = starting_grove();
        let east = attach_room(
            &parent,
            Direction::East,
            DEFAULT_EW_ROOM_WIDTH,
            DEFAULT_NS_ROOM_HEIGHT,
        );
        assert_eq!(east.y, parent.y);
        assert_eq!(east.height, parent.height);
        assert!((east.min_x() - parent.max_x()).abs() < SNAP_EPS);
    }
}
