use serde::Deserialize;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Deserialize)]
pub enum Rarity {
    #[default]
    Common,
    Uncommon,
    Rare,
    Legendary,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Deserialize)]
pub enum ItemType {
    #[default]
    Block,
    Resource,
    Tool,
    Weapon,
    Armor,
    Consumable,
    Material,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
pub enum EquipmentSlot {
    Head,
    Chest,
    Legs,
    Back,
    Accessory1,
    Accessory2,
    Accessory3,
    Accessory4,
    Weapon1,
    Weapon2,
    Pet,
    CosmeticHead,
    CosmeticChest,
    CosmeticLegs,
    CosmeticBack,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ItemStats {
    pub damage: Option<f32>,
    pub defense: Option<f32>,
    pub speed_bonus: Option<f32>,
    pub health_bonus: Option<i32>,
}

fn default_max_stack() -> u16 {
    99
}

#[derive(Debug, Clone, Deserialize)]
pub struct ItemDef {
    pub id: String,
    pub display_name: String,
    pub description: String,
    #[serde(default = "default_max_stack")]
    pub max_stack: u16,
    #[serde(default)]
    pub rarity: Rarity,
    #[serde(default)]
    pub item_type: ItemType,
    pub icon: String,
    pub placeable: Option<String>,
    pub equipment_slot: Option<EquipmentSlot>,
    pub stats: Option<ItemStats>,
}

fn default_drop_min() -> u16 {
    1
}
fn default_drop_max() -> u16 {
    1
}
fn default_drop_chance() -> f32 {
    1.0
}

#[derive(Debug, Clone, Deserialize)]
pub struct DropDef {
    pub item_id: String,
    #[serde(default = "default_drop_min")]
    pub min: u16,
    #[serde(default = "default_drop_max")]
    pub max: u16,
    #[serde(default = "default_drop_chance")]
    pub chance: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn item_def_has_required_fields() {
        let item = ItemDef {
            id: "dirt".into(),
            display_name: "Dirt Block".into(),
            description: "A block of dirt".into(),
            max_stack: 999,
            rarity: Rarity::Common,
            item_type: ItemType::Block,
            icon: "items/dirt.png".into(),
            placeable: Some("dirt".into()),
            equipment_slot: None,
            stats: None,
        };

        assert_eq!(item.id, "dirt");
        assert_eq!(item.max_stack, 999);
        assert!(item.placeable.is_some());
    }

    #[test]
    fn drop_def_calculates_count() {
        let drop = DropDef {
            item_id: "dirt".into(),
            min: 1,
            max: 3,
            chance: 1.0,
        };

        assert!(drop.min <= drop.max);
        assert!(drop.chance <= 1.0);
    }
}
