//! A typed registration as the fork's content declaration: the one mapping both the game bridge
//! (`mod/src/content.rs`) and the offline bake (`tools/bake.py`, through the probe's `content`
//! object) use.
//!
//! The runtime records what a jar's `Registry.register` calls built — an item's
//! `Item.Properties` chain, a block's `BlockBehaviour.Properties` chain — as typed
//! [`RegistryDeclaration`]s with the property strings the host understood. This module turns
//! those into the narrow vocabulary a content package declares
//! (`data/<namespace>/horizon/{block,item}/<path>.json`): the template plus the property keys the
//! fork reads. Anything else stays in the jar's row and is mapped in `docs/m3-content-design.md`,
//! never guessed into content.

use std::collections::BTreeMap;

use serde_json::{Map, Value, json};

use crate::{RegistryDeclaration, RegistryEntry};

/// `content::Rarity`, by its file spelling.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rarity {
    Common,
    Uncommon,
    Rare,
    Epic,
}

impl Rarity {
    pub fn name(self) -> &'static str {
        match self {
            Self::Common => "common",
            Self::Uncommon => "uncommon",
            Self::Rare => "rare",
            Self::Epic => "epic",
        }
    }

    fn parse(name: &str) -> Option<Self> {
        match name.to_ascii_lowercase().as_str() {
            "common" => Some(Self::Common),
            "uncommon" => Some(Self::Uncommon),
            "rare" => Some(Self::Rare),
            "epic" => Some(Self::Epic),
            _ => None,
        }
    }
}

/// A block a jar registered, in the content vocabulary.
#[derive(Clone, Debug, PartialEq)]
pub struct Block {
    pub id: String,
    /// The vanilla block it copies, `minecraft:<path>`.
    pub template: String,
    /// Whether it brings a block item of the same id.
    pub item: bool,
    pub hardness: Option<f32>,
    pub explosion_resistance: Option<f32>,
    pub friction: Option<f32>,
    pub requires_tool: Option<bool>,
}

/// An item a jar registered, in the content vocabulary.
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub id: String,
    /// The vanilla item it copies, `minecraft:<path>`.
    pub template: String,
    pub max_stack_size: Option<u8>,
    pub rarity: Option<Rarity>,
    pub crafting_remainder: Option<String>,
}

/// One block or item a jar registered.
#[derive(Clone, Debug, PartialEq)]
pub enum ContentDeclaration {
    Block(Block),
    Item(Item),
}

impl ContentDeclaration {
    pub fn id(&self) -> &str {
        match self {
            Self::Block(block) => &block.id,
            Self::Item(item) => &item.id,
        }
    }

    /// The namespace half of the id.
    pub fn namespace(&self) -> &str {
        self.id()
            .split_once(':')
            .map_or("", |(namespace, _)| namespace)
    }

    /// The path half of the id: the declaration file's name.
    pub fn path(&self) -> &str {
        self.id().split_once(':').map_or("", |(_, path)| path)
    }

    /// Whether the declaration is a block.
    pub fn is_block(&self) -> bool {
        matches!(self, Self::Block(_))
    }

    /// The `data/<namespace>/horizon/{block,item}/<path>.json` value the content loader reads
    /// (`horizon-mod-package/src/content.rs`, `BlockFile`/`ItemFile`).
    pub fn file_json(&self) -> Value {
        match self {
            Self::Block(block) => {
                let mut properties = Map::new();
                if let Some(value) = block.hardness {
                    properties.insert("hardness".into(), json!(value));
                }
                if let Some(value) = block.explosion_resistance {
                    properties.insert("explosion_resistance".into(), json!(value));
                }
                if let Some(value) = block.friction {
                    properties.insert("friction".into(), json!(value));
                }
                if let Some(value) = block.requires_tool {
                    properties.insert("requires_tool".into(), json!(value));
                }
                json!({
                    "template": block.template,
                    "properties": Value::Object(properties),
                    "item": block.item,
                })
            }
            Self::Item(item) => {
                let mut value = json!({ "template": item.template });
                if let Some(size) = item.max_stack_size {
                    value["max_stack_size"] = json!(size);
                }
                if let Some(rarity) = item.rarity {
                    value["rarity"] = json!(rarity.name());
                }
                if let Some(remainder) = &item.crafting_remainder {
                    value["crafting_remainder"] = json!(remainder);
                }
                value
            }
        }
    }
}

/// Every typed block and item registration of a jar, in registration order.
pub fn declarations(entries: &[RegistryEntry]) -> Vec<ContentDeclaration> {
    entries
        .iter()
        .filter_map(|entry| match &entry.declaration {
            Some(RegistryDeclaration::Block {
                template,
                properties,
                item,
            }) => Some(ContentDeclaration::Block(Block {
                id: entry.id.clone(),
                template: template.clone(),
                item: *item,
                hardness: number(properties, "hardness"),
                explosion_resistance: number(properties, "explosion_resistance"),
                friction: number(properties, "friction"),
                requires_tool: properties.get("requires_tool").map(|value| value == "true"),
            })),
            Some(RegistryDeclaration::Item {
                template,
                properties,
            }) => Some(ContentDeclaration::Item(Item {
                id: entry.id.clone(),
                template: template.clone(),
                max_stack_size: properties
                    .get("max_stack_size")
                    .and_then(|value| value.parse().ok())
                    .filter(|size| (1..=99).contains(size)),
                rarity: properties
                    .get("rarity")
                    .and_then(|rarity| Rarity::parse(rarity)),
                crafting_remainder: properties.get("crafting_remainder").cloned(),
            })),
            _ => None,
        })
        .collect()
}

fn number(properties: &BTreeMap<String, String>, key: &str) -> Option<f32> {
    properties.get(key).and_then(|value| value.parse().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, declaration: RegistryDeclaration) -> RegistryEntry {
        RegistryEntry {
            registry: "minecraft:item".into(),
            id: id.into(),
            value: crate::Value::Void,
            declaration: Some(declaration),
        }
    }

    /// The content file is the vocabulary the fork reads, with only the keys the runtime
    /// recorded: a block's properties, and an item's stack size, rarity and remainder.
    #[test]
    fn declarations_become_the_content_file_json() {
        let properties = BTreeMap::from([
            ("hardness".to_owned(), "3".to_owned()),
            ("explosion_resistance".to_owned(), "3".to_owned()),
            ("requires_tool".to_owned(), "true".to_owned()),
            ("light".to_owned(), "7".to_owned()),
        ]);
        let entries = vec![
            entry(
                "m3_fixture:ruby_block",
                RegistryDeclaration::Block {
                    template: "minecraft:stone".into(),
                    properties: properties.clone(),
                    item: true,
                },
            ),
            entry(
                "m3_fixture:ruby",
                RegistryDeclaration::Item {
                    template: "minecraft:stick".into(),
                    properties: BTreeMap::from([
                        ("max_stack_size".to_owned(), "16".to_owned()),
                        ("rarity".to_owned(), "rare".to_owned()),
                        (
                            "crafting_remainder".to_owned(),
                            "minecraft:paper".to_owned(),
                        ),
                    ]),
                },
            ),
        ];
        let declarations = declarations(&entries);
        let block = declarations[0].file_json();
        assert_eq!(block["template"], "minecraft:stone");
        assert_eq!(block["item"], true);
        assert_eq!(block["properties"]["hardness"], 3.0);
        assert_eq!(block["properties"]["requires_tool"], true);
        // `light` has no content key; the declaration does not invent one.
        assert!(block["properties"].get("light").is_none());
        let item = declarations[1].file_json();
        assert_eq!(item["template"], "minecraft:stick");
        assert_eq!(item["max_stack_size"], 16);
        assert_eq!(item["rarity"], "rare");
        assert_eq!(item["crafting_remainder"], "minecraft:paper");
        assert_eq!(declarations[1].namespace(), "m3_fixture");
        assert_eq!(declarations[1].path(), "ruby");
    }

    /// A recorded stack size outside the content vocabulary's range is not carried, as the
    /// bridge has always refused it.
    #[test]
    fn an_out_of_range_stack_size_is_left_out() {
        let entries = vec![entry(
            "m3_fixture:ruby",
            RegistryDeclaration::Item {
                template: "minecraft:stick".into(),
                properties: BTreeMap::from([("max_stack_size".to_owned(), "1000".to_owned())]),
            },
        )];
        assert!(declarations(&entries)[0].file_json()["max_stack_size"].is_null());
    }
}
