use crate::inventory::InventorySlot;

use super::definition::ObjectId;

/// Reference from an occupancy grid cell to the object that occupies it.
#[derive(Debug, Clone, Copy)]
pub struct OccupancyRef {
    pub object_index: u16,
    pub is_anchor: bool,
}

/// State of a placed object (varies by ObjectType).
#[derive(Debug, Clone, Default)]
pub enum ObjectState {
    #[default]
    Default,
    Container {
        contents: Vec<Option<InventorySlot>>,
    },
}

/// A single object placed in a chunk, stored in ChunkData.
#[derive(Debug, Clone)]
pub struct PlacedObject {
    pub object_id: ObjectId,
    pub local_x: u32,
    pub local_y: u32,
    pub state: ObjectState,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placed_object_stores_position() {
        let obj = PlacedObject {
            object_id: ObjectId(1),
            local_x: 5,
            local_y: 10,
            state: ObjectState::Default,
        };
        assert_eq!(obj.local_x, 5);
        assert_eq!(obj.local_y, 10);
    }

    #[test]
    fn object_state_default() {
        let state = ObjectState::Default;
        assert!(matches!(state, ObjectState::Default));
    }

    #[test]
    fn object_state_container() {
        let state = ObjectState::Container {
            contents: vec![None; 16],
        };
        match state {
            ObjectState::Container { contents } => assert_eq!(contents.len(), 16),
            _ => panic!("expected Container"),
        }
    }

    #[test]
    fn occupancy_ref_tracks_anchor() {
        let occ = OccupancyRef {
            object_index: 0,
            is_anchor: true,
        };
        assert!(occ.is_anchor);
        assert_eq!(occ.object_index, 0);
    }
}
