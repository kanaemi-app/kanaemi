//! The values IBus sends over D-Bus. Each is a structure that starts with
//! its type's name and a dictionary of attachments, which IBus calls a
//! serializable; the fields of the type follow.

use std::collections::HashMap;

use zvariant::{StructureBuilder, Value};

use kanaemi_linux::reply::Item;

/// An underline attribute, and its value for none.
const ATTR_UNDERLINE: u32 = 1;
const UNDERLINE_NONE: u32 = 0;
/// A foreground colour attribute, and the grey of what follows a candidate.
const ATTR_FOREGROUND: u32 = 2;
const MUTED: u32 = 0x80_80_80;
/// The grey on the selected candidate: panels keep a text's own colour on the
/// highlight, where the plain grey sinks into it.
const MUTED_SELECTED: u32 = 0xd8_d8_d8;
const PROP_TYPE_NORMAL: u32 = 0;
const PROP_STATE_UNCHECKED: u32 = 0;

fn serializable(name: &str) -> StructureBuilder<'static> {
    StructureBuilder::new()
        .add_field(name.to_owned())
        .add_field(HashMap::<String, Value<'static>>::new())
}

fn build(builder: StructureBuilder<'static>) -> Value<'static> {
    Value::Structure(builder.build().expect("an IBus value has fields"))
}

/// Text with no attributes.
pub fn text(text: &str) -> Value<'static> {
    text_with(text, Vec::new())
}

/// Text to show as the preedit. Its marks show its state, so it is not
/// underlined as clients underline a preedit by default.
pub fn preedit(text: &str) -> Value<'static> {
    let end = text.chars().count() as u32;
    let plain = build(
        serializable("IBusAttribute")
            .add_field(ATTR_UNDERLINE)
            .add_field(UNDERLINE_NONE)
            .add_field(0u32)
            .add_field(end),
    );
    text_with(text, vec![plain])
}

fn text_with(text: &str, attributes: Vec<Value<'static>>) -> Value<'static> {
    let list = build(serializable("IBusAttrList").add_field(attributes));
    build(
        serializable("IBusText")
            .add_field(text.to_owned())
            .add_field(list),
    )
}

/// A candidate's text, what follows the candidate greyed. A panel that
/// draws no attributes, as GNOME Shell's, shows it all alike.
fn candidate(item: &Item, selected: bool) -> Value<'static> {
    let Some(start) = item.muted_from else {
        return text(&item.text);
    };
    let end = item.text.chars().count() as u32;
    let muted = build(
        serializable("IBusAttribute")
            .add_field(ATTR_FOREGROUND)
            .add_field(if selected { MUTED_SELECTED } else { MUTED })
            .add_field(start)
            .add_field(end),
    );
    text_with(&item.text, vec![muted])
}

/// A page of candidates, numbered from 1, with `selected` highlighted.
pub fn lookup_table(items: &[Item], selected: usize) -> Value<'static> {
    let candidates: Vec<Value<'static>> = items
        .iter()
        .enumerate()
        .map(|(i, item)| candidate(item, i == selected))
        .collect();
    let labels: Vec<Value<'static>> = (1..=items.len()).map(|n| text(&n.to_string())).collect();
    build(
        serializable("IBusLookupTable")
            .add_field(items.len().max(1) as u32)
            .add_field(selected as u32)
            .add_field(true)
            .add_field(false)
            // The panel's own orientation.
            .add_field(-1i32)
            .add_field(candidates)
            .add_field(labels),
    )
}

/// An item of the engine's menu, which the panel lists under the input
/// source.
pub struct Property<'a> {
    pub key: &'a str,
    pub label: &'a str,
}

fn property(property: &Property) -> Value<'static> {
    build(
        serializable("IBusProperty")
            .add_field(property.key.to_owned())
            .add_field(PROP_TYPE_NORMAL)
            .add_field(text(property.label))
            .add_field(String::new())
            .add_field(text(property.label))
            .add_field(true)
            .add_field(true)
            .add_field(PROP_STATE_UNCHECKED)
            .add_field(property_list(&[]))
            // No symbol: a panel such as GNOME's shows a property's symbol in
            // its top bar as the input mode, which the IME shows itself.
            .add_field(text("")),
    )
}

pub fn property_list(properties: &[Property]) -> Value<'static> {
    let properties: Vec<Value<'static>> = properties.iter().map(property).collect();
    build(serializable("IBusPropList").add_field(properties))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signature(value: &Value) -> String {
        value.value_signature().to_string()
    }

    fn fields(value: Value<'static>) -> Vec<Value<'static>> {
        match value {
            Value::Structure(structure) => structure.into_fields(),
            other => panic!("not a structure: {other:?}"),
        }
    }

    #[test]
    fn text_is_a_serializable_with_its_attributes() {
        assert_eq!(signature(&text("かな")), "(sa{sv}sv)");
        assert_eq!(signature(&preedit("›かな")), "(sa{sv}sv)");
        let text = fields(text("かな"));
        assert_eq!(text[0], Value::from("IBusText"));
        assert_eq!(text[2], Value::from("かな"));
    }

    #[test]
    fn the_preedit_is_not_underlined_from_its_first_character_to_its_last() {
        let preedit = fields(preedit("›かな"));
        let Value::Value(list) = &preedit[3] else {
            panic!("the attribute list is a variant");
        };
        let list = fields((**list).try_clone().unwrap());
        let Value::Array(attributes) = &list[2] else {
            panic!("the attributes are an array");
        };
        let [Value::Value(attribute)] = attributes.inner() else {
            panic!("one attribute");
        };
        let attribute = fields((**attribute).try_clone().unwrap());
        assert_eq!(
            attribute[2..],
            [
                Value::from(ATTR_UNDERLINE),
                Value::from(UNDERLINE_NONE),
                Value::from(0u32),
                Value::from(3u32),
            ],
            "counted in characters"
        );
    }

    fn attributes_of(text: Value<'static>) -> Vec<Vec<Value<'static>>> {
        let text = fields(text);
        let Value::Value(list) = &text[3] else {
            panic!("the attribute list is a variant");
        };
        let list = fields((**list).try_clone().unwrap());
        let Value::Array(attributes) = &list[2] else {
            panic!("the attributes are an array");
        };
        attributes
            .inner()
            .iter()
            .map(|attribute| {
                let Value::Value(attribute) = attribute else {
                    panic!("an attribute is a variant");
                };
                fields((**attribute).try_clone().unwrap())
            })
            .collect()
    }

    fn item(text: &str, muted_from: Option<u32>) -> Item {
        Item {
            text: text.to_owned(),
            muted_from,
        }
    }

    #[test]
    fn what_follows_a_candidate_is_greyed_to_its_end() {
        let text = candidate(&item("漢字　ユーザー辞書", Some(2)), false);
        assert_eq!(
            fields(text.try_clone().unwrap())[2],
            Value::from("漢字　ユーザー辞書")
        );
        let attributes = attributes_of(text);
        assert_eq!(attributes.len(), 1);
        assert_eq!(
            attributes[0][2..],
            [
                Value::from(ATTR_FOREGROUND),
                Value::from(MUTED),
                Value::from(2u32),
                Value::from(9u32),
            ],
            "counted in characters"
        );
    }

    #[test]
    fn a_candidate_alone_has_no_attributes() {
        assert!(attributes_of(candidate(&item("漢字", None), false)).is_empty());
    }

    #[test]
    fn what_follows_the_selected_candidate_is_greyed_light_against_the_highlight() {
        let table = fields(lookup_table(
            &[
                item("漢字　ユーザー辞書", Some(2)),
                item("感じ　ユーザー辞書", Some(2)),
            ],
            1,
        ));
        let Value::Array(candidates) = &table[7] else {
            panic!("the candidates are an array");
        };
        let colour = |i: usize| {
            let Value::Value(text) = &candidates.inner()[i] else {
                panic!("a candidate is a variant");
            };
            attributes_of((**text).try_clone().unwrap())[0][3]
                .try_clone()
                .unwrap()
        };
        assert_eq!(colour(0), Value::from(MUTED));
        assert_eq!(colour(1), Value::from(MUTED_SELECTED));
    }

    #[test]
    fn a_lookup_table_has_its_page_and_labels() {
        let table = lookup_table(&[item("漢字", None), item("感じ", None)], 1);
        assert_eq!(signature(&table), "(sa{sv}uubbiavav)");
        let table = fields(table);
        assert_eq!(table[2..4], [Value::from(2u32), Value::from(1u32)]);
    }

    #[test]
    fn a_property_has_no_symbol_to_show_as_a_mode() {
        let settings = Property {
            key: "settings",
            label: "設定を開く…",
        };
        assert_eq!(signature(&property(&settings)), "(sa{sv}suvsvbbuvv)");
        assert_eq!(signature(&property_list(&[settings])), "(sa{sv}av)");
        let symbol = fields(property(&Property {
            key: "settings",
            label: "設定を開く…",
        }))
        .pop()
        .unwrap();
        let Value::Value(symbol) = symbol else {
            panic!("the symbol is a variant");
        };
        assert_eq!(fields((*symbol).try_clone().unwrap())[2], Value::from(""));
    }
}
