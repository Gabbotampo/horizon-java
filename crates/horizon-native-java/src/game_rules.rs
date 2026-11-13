//! Vanilla 26.3's game rules, as the listener façade answers them.
//!
//! In 26.3 a rule is a `GameRule<T>` registered in `BuiltInRegistries.GAME_RULE`
//! by `GameRules.<clinit>`, one `public static final` field per rule, and a
//! world's `GameRules` maps each enabled rule to its boxed value. The façade
//! stands a `game-rule/<id>` marker in for each field and answers the reads
//! mods make on it and on `getGameRules()` (`facade_server.rs`):
//!
//! - **The rule itself** (no world needed): `id()`, `toString()`,
//!   `getIdentifier()`, `getDescriptionId()`, `defaultValue()`,
//!   `serialize(value)` and `getCommandResult(value)`, from [`VANILLA_RULES`].
//! - **The world's value**: `GameRules.get(rule)` and `getAsString(rule)`
//!   answer the value the game holds now ([`crate::ServerFacts::game_rules`],
//!   which the game refreshes whenever a rule changes).
//!
//! [`VANILLA_RULES`] is the 26.3 server jar's `GameRules.<clinit>`, in field
//! order. `game_rules_oracle.tsv` is the same list printed by
//! the real classes on JDK 25 (`tests/fixtures/game_rules/GameRulesOracle.java`), and the tests
//! check every row of the table against it. A `GameRules` field that is not in
//! the table (a rule of another version) is not modelled: the static read is
//! left to the caller, which counts it.

use crate::{MethodRef, NativeError, Value, boxed_boolean, boxed_integer, identifier_parse};

/// A game rule's value as the game holds it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameRuleValue {
    Bool(bool),
    Int(i32),
}

impl GameRuleValue {
    /// The boxed `Boolean`/`Integer` a `GameRule<T>` read answers.
    pub(crate) fn boxed(self) -> Value {
        match self {
            Self::Bool(value) => crate::boolean_value(value),
            Self::Int(value) => Value::Boxed(crate::Boxed::Integer(value)),
        }
    }

    /// `GameRule.serialize(value)`, which is `value.toString()`.
    pub(crate) fn serialized(self) -> String {
        match self {
            Self::Bool(value) => value.to_string(),
            Self::Int(value) => value.to_string(),
        }
    }

    /// `GameRule.getCommandResult(value)`: `registerBoolean`'s `b ? 1 : 0`,
    /// `registerInteger`'s `i -> i`.
    pub(crate) fn command_result(self) -> i32 {
        match self {
            Self::Bool(value) => i32::from(value),
            Self::Int(value) => value,
        }
    }
}

/// One vanilla rule: its field (whose lower-case name is its id), its default
/// value, and whether it needs a feature flag the vanilla set lacks
/// (`max_minecart_speed` needs `minecart_improvements`), so a world without
/// that flag has no value for it.
pub(crate) struct VanillaRule {
    pub field: &'static str,
    pub default: GameRuleValue,
    pub feature_gated: bool,
}

const fn boolean(field: &'static str, default: bool) -> VanillaRule {
    VanillaRule {
        field,
        default: GameRuleValue::Bool(default),
        feature_gated: false,
    }
}

const fn integer(field: &'static str, default: i32) -> VanillaRule {
    VanillaRule {
        field,
        default: GameRuleValue::Int(default),
        feature_gated: false,
    }
}

/// `GameRules.<clinit>` of the 26.3 server jar, in field order. The two
/// `advance_*` defaults are `!SharedConstants.DEBUG_WORLD_RECREATE`, which is
/// `false` in a release, so `true`.
pub(crate) const VANILLA_RULES: &[VanillaRule] = &[
    boolean("ADVANCE_TIME", true),
    boolean("ADVANCE_WEATHER", true),
    boolean("ALLOW_ENTERING_NETHER_USING_PORTALS", true),
    boolean("BLOCK_DROPS", true),
    boolean("BLOCK_EXPLOSION_DROP_DECAY", true),
    boolean("COMMAND_BLOCKS_WORK", true),
    boolean("COMMAND_BLOCK_OUTPUT", true),
    boolean("DROWNING_DAMAGE", true),
    boolean("ELYTRA_MOVEMENT_CHECK", true),
    boolean("ENDER_PEARLS_VANISH_ON_DEATH", true),
    boolean("ENTITY_DROPS", true),
    boolean("FALL_DAMAGE", true),
    boolean("FIRE_DAMAGE", true),
    integer("FIRE_SPREAD_RADIUS_AROUND_PLAYER", 128),
    boolean("FORGIVE_DEAD_PLAYERS", true),
    boolean("FREEZE_DAMAGE", true),
    boolean("GLOBAL_SOUND_EVENTS", true),
    boolean("IMMEDIATE_RESPAWN", false),
    boolean("KEEP_INVENTORY", false),
    boolean("LAVA_SOURCE_CONVERSION", false),
    boolean("LIMITED_CRAFTING", false),
    boolean("LOCATOR_BAR", true),
    boolean("LOG_ADMIN_COMMANDS", true),
    integer("MAX_BLOCK_MODIFICATIONS", 32768),
    integer("MAX_COMMAND_FORKS", 65536),
    integer("MAX_COMMAND_SEQUENCE_LENGTH", 65536),
    integer("MAX_ENTITY_CRAMMING", 24),
    VanillaRule {
        field: "MAX_MINECART_SPEED",
        default: GameRuleValue::Int(8),
        feature_gated: true,
    },
    integer("MAX_SNOW_ACCUMULATION_HEIGHT", 1),
    boolean("MOB_DROPS", true),
    boolean("MOB_EXPLOSION_DROP_DECAY", true),
    boolean("MOB_GRIEFING", true),
    boolean("NATURAL_HEALTH_REGENERATION", true),
    boolean("PLAYER_MOVEMENT_CHECK", true),
    integer("PLAYERS_NETHER_PORTAL_CREATIVE_DELAY", 0),
    integer("PLAYERS_NETHER_PORTAL_DEFAULT_DELAY", 80),
    integer("PLAYERS_SLEEPING_PERCENTAGE", 100),
    boolean("PROJECTILES_CAN_BREAK_BLOCKS", true),
    boolean("PVP", true),
    boolean("RAIDS", true),
    integer("RANDOM_TICK_SPEED", 3),
    boolean("REDUCED_DEBUG_INFO", false),
    integer("RESPAWN_RADIUS", 10),
    boolean("SEND_COMMAND_FEEDBACK", true),
    boolean("SHOW_ADVANCEMENT_MESSAGES", true),
    boolean("SHOW_DEATH_MESSAGES", true),
    boolean("SPAWNER_BLOCKS_WORK", true),
    boolean("SPAWN_MOBS", true),
    boolean("SPAWN_MONSTERS", true),
    boolean("SPAWN_PATROLS", true),
    boolean("SPAWN_PHANTOMS", true),
    boolean("SPAWN_WANDERING_TRADERS", true),
    boolean("SPAWN_WARDENS", true),
    boolean("SPECTATORS_GENERATE_CHUNKS", true),
    boolean("SPREAD_VINES", true),
    boolean("TNT_EXPLODES", true),
    boolean("TNT_EXPLOSION_DROP_DECAY", false),
    boolean("UNIVERSAL_ANGER", false),
    boolean("WATER_SOURCE_CONVERSION", true),
];

/// The vanilla rule whose `GameRules` field is `field`.
pub(crate) fn by_field(field: &str) -> Option<&'static VanillaRule> {
    VANILLA_RULES.iter().find(|rule| rule.field == field)
}

/// The vanilla rule whose id (`keep_inventory`) is `id`.
pub(crate) fn by_id(id: &str) -> Option<&'static VanillaRule> {
    VANILLA_RULES
        .iter()
        .find(|rule| rule.field.len() == id.len() && rule.field.eq_ignore_ascii_case(id))
}

/// `value` read as an argument of a rule whose values have `default`'s type:
/// `serialize(T)` and `getCommandResult(T)` take the boxed value. `None` for
/// any other value (a `null`, a box of the other type), which the caller
/// refuses rather than guess vanilla's `ClassCastException`/NPE.
fn argument(default: GameRuleValue, value: &Value) -> Option<GameRuleValue> {
    match default {
        GameRuleValue::Bool(_) => boxed_boolean(value).map(GameRuleValue::Bool),
        GameRuleValue::Int(_) => match value {
            // `serialize` takes an `Object`, so this is a box: an `Integer`
            // from `valueOf`, or one the runtime holds as its value (read
            // back from a collection).
            Value::Boxed(_) | Value::Int(_) => boxed_integer(value).map(GameRuleValue::Int),
            _ => None,
        },
    }
}

/// A member of a rule marker (`game-rule/<id>`), answered from the rule alone.
/// `None` when `r` is not one this models, so the caller refuses it by name.
pub(crate) fn rule_member(id: &str, r: &MethodRef, args: &[Value]) -> Option<Value> {
    let rule = by_id(id)?;
    let value = |index: usize| args.get(index).and_then(|v| argument(rule.default, v));
    Some(match (r.name.as_str(), r.descriptor.as_str()) {
        // `getIdentifier().toShortString()`: vanilla's ids are all in the
        // `minecraft` namespace, which `toShortString` drops.
        ("id" | "toString", "()Ljava/lang/String;") => Value::Ref(Some(id.to_owned())),
        ("getIdentifier", "()Lnet/minecraft/resources/Identifier;") => {
            identifier_parse(&format!("minecraft:{id}"))
        }
        // `Util.makeDescriptionId("gamerule", id)`.
        ("getDescriptionId", "()Ljava/lang/String;") => {
            Value::Ref(Some(format!("gamerule.minecraft.{id}")))
        }
        ("defaultValue", "()Ljava/lang/Object;") => rule.default.boxed(),
        ("serialize", "(Ljava/lang/Object;)Ljava/lang/String;") => {
            Value::Ref(Some(value(0)?.serialized()))
        }
        ("getCommandResult", "(Ljava/lang/Object;)I") => Value::Int(value(0)?.command_result()),
        _ => return None,
    })
}

/// What `GameRules.get(rule)` and `getAsString(rule)` do for a vanilla rule
/// the world has no value for: a feature-gated rule is absent from a world
/// without its flag, and vanilla's `get` throws. `None` for a rule the world
/// should have: its absence is a gap in what the game passed, which the
/// caller refuses.
pub(crate) fn missing_value(id: &str) -> Option<NativeError> {
    by_id(id)
        .filter(|rule| rule.feature_gated)
        .map(|_| NativeError::UncaughtException {
            class: "java/lang/IllegalArgumentException".into(),
            message: Some(format!("Tried to access invalid game rule minecraft:{id}")),
            frame: None,
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    const ORACLE: &str = include_str!("game_rules_oracle.tsv");

    fn call(name: &str, descriptor: &str) -> MethodRef {
        MethodRef {
            owner: "net/minecraft/world/level/gamerules/GameRule".into(),
            name: name.into(),
            descriptor: descriptor.into(),
        }
    }

    fn text(value: Option<Value>) -> String {
        match value {
            Some(Value::Ref(Some(text))) => text,
            other => panic!("not a string: {other:?}"),
        }
    }

    /// The oracle's rows: every `GameRule` field of the real 26.3
    /// `GameRules`, with what each read answers on JDK 25.
    fn oracle_rows() -> Vec<Vec<&'static str>> {
        ORACLE
            .lines()
            .filter(|line| !line.starts_with('#') && !line.is_empty())
            .map(|line| line.split('\t').collect())
            .filter(|row: &Vec<&str>| row[0].bytes().all(|b| b.is_ascii_uppercase() || b == b'_'))
            .collect()
    }

    /// Every field the real `GameRules` declares is in the table, in the
    /// same order, and each rule's own reads answer what the real rule
    /// answers: `id()`, `toString()`, `getDescriptionId()`, `defaultValue()`
    /// (type and value), `serialize(default)` and `getCommandResult(default)`.
    #[test]
    fn every_vanilla_rule_answers_its_own_reads_as_the_real_class() {
        let rows = oracle_rows();
        assert_eq!(rows.len(), 59, "the 26.3 jar declares 59 rules");
        let fields: Vec<&str> = VANILLA_RULES.iter().map(|rule| rule.field).collect();
        let oracle_fields: Vec<&str> = rows.iter().map(|row| row[0]).collect();
        assert_eq!(fields, oracle_fields);
        for row in rows {
            let [
                field,
                id,
                identifier,
                to_string,
                description,
                kind,
                default,
                serialized,
                result,
                gated,
                ..,
            ] = row[..]
            else {
                panic!("short row {row:?}");
            };
            let rule = by_field(field).unwrap();
            let ask = |name: &str, descriptor: &str, args: &[Value]| {
                rule_member(id, &call(name, descriptor), args)
            };
            assert_eq!(text(ask("id", "()Ljava/lang/String;", &[])), id);
            assert_eq!(
                text(ask("toString", "()Ljava/lang/String;", &[])),
                to_string
            );
            assert_eq!(
                ask(
                    "getIdentifier",
                    "()Lnet/minecraft/resources/Identifier;",
                    &[]
                ),
                Some(identifier_parse(identifier))
            );
            assert_eq!(
                text(ask("getDescriptionId", "()Ljava/lang/String;", &[])),
                description
            );
            let boxed = ask("defaultValue", "()Ljava/lang/Object;", &[]).unwrap();
            let (class, value) = default.split_once(':').unwrap();
            let expected = match class {
                "java.lang.Boolean" => crate::Boxed::Boolean(value == "true"),
                "java.lang.Integer" => crate::Boxed::Integer(value.parse().unwrap()),
                other => panic!("unexpected default class {other}"),
            };
            assert_eq!(boxed, Value::Boxed(expected), "{field}");
            assert_eq!(
                kind,
                match rule.default {
                    GameRuleValue::Bool(_) => "BOOL",
                    GameRuleValue::Int(_) => "INT",
                }
            );
            assert_eq!(
                text(ask(
                    "serialize",
                    "(Ljava/lang/Object;)Ljava/lang/String;",
                    std::slice::from_ref(&boxed)
                )),
                serialized
            );
            assert_eq!(
                ask("getCommandResult", "(Ljava/lang/Object;)I", &[boxed]),
                Some(Value::Int(result.parse().unwrap()))
            );
            assert_eq!(rule.feature_gated.to_string(), gated, "{field}");
        }
    }

    /// `serialize` and `getCommandResult` of values other than the default
    /// (the oracle's `serialize-false` and `serialize-int` rows).
    #[test]
    fn serialize_and_command_result_follow_the_value_given() {
        let row = |name: &str| -> Vec<&'static str> {
            ORACLE
                .lines()
                .find(|line| line.starts_with(&format!("{name}\t")))
                .unwrap()
                .split('\t')
                .collect()
        };
        let serialize = call("serialize", "(Ljava/lang/Object;)Ljava/lang/String;");
        let result = call("getCommandResult", "(Ljava/lang/Object;)I");
        let off = crate::boolean_value(false);
        let expected = row("serialize-false");
        assert_eq!(
            text(rule_member(
                "keep_inventory",
                &serialize,
                std::slice::from_ref(&off)
            )),
            expected[1]
        );
        assert_eq!(
            rule_member("keep_inventory", &result, &[off]),
            Some(Value::Int(expected[2].parse().unwrap()))
        );
        let minus_seven = Value::Boxed(crate::Boxed::Integer(-7));
        let expected = row("serialize-int");
        assert_eq!(
            text(rule_member(
                "random_tick_speed",
                &serialize,
                std::slice::from_ref(&minus_seven)
            )),
            expected[1]
        );
        assert_eq!(
            rule_member("random_tick_speed", &result, &[minus_seven]),
            Some(Value::Int(expected[2].parse().unwrap()))
        );
        // A box of the other type, or null, is not modelled (vanilla would
        // throw a `ClassCastException` or NPE from the codec's lambda).
        assert_eq!(
            rule_member(
                "random_tick_speed",
                &serialize,
                &[crate::boolean_value(true)]
            ),
            None
        );
        assert_eq!(
            rule_member("keep_inventory", &result, &[Value::Ref(None)]),
            None
        );
    }

    /// A vanilla-features world has no `max_minecart_speed`, and `get` throws
    /// exactly what the real `GameRules.get` throws (the oracle's
    /// `get-gated` row). Any other rule's absence is not vanilla's, so it is
    /// left to the caller.
    #[test]
    fn a_feature_gated_rule_a_world_lacks_throws_as_vanilla() {
        let expected: Vec<&str> = ORACLE
            .lines()
            .find(|line| line.starts_with("get-gated\t"))
            .unwrap()
            .split('\t')
            .collect();
        let Some(NativeError::UncaughtException { class, message, .. }) =
            missing_value("max_minecart_speed")
        else {
            panic!("max_minecart_speed should throw");
        };
        assert_eq!(class.replace('/', "."), expected[1]);
        assert_eq!(message.as_deref(), Some(expected[2]));
        assert!(missing_value("keep_inventory").is_none());
        assert!(missing_value("not_a_rule").is_none());
    }
}
