use super::*;
use crate::scalar;

fn s(v: &str) -> Value {
    Value::Str(v.to_string())
}

#[test]
fn scalars_quote_like_yaml_v3() {
    assert_eq!(scalar("abc"), "abc");
    assert_eq!(scalar("/tmp/a b"), "/tmp/a b");
    assert_eq!(scalar(""), "\"\"");
    assert_eq!(scalar("true"), "\"true\"");
    assert_eq!(scalar("123"), "\"123\"");
    assert_eq!(scalar("1.5"), "\"1.5\"");
    assert_eq!(scalar("1.2.3"), "1.2.3");
    assert_eq!(scalar("null"), "\"null\"");
    assert_eq!(scalar("yes"), "\"yes\"");
    assert_eq!(scalar("a: b"), "'a: b'");
    assert_eq!(scalar("x #y"), "'x #y'");
    assert_eq!(scalar(" lead"), "' lead'");
    assert_eq!(scalar("it's: x"), "'it''s: x'");
    assert_eq!(scalar("- x"), "'- x'");
    assert_eq!(scalar("-x"), "-x");
    assert_eq!(scalar("a\tb"), "\"a\\tb\"");
    assert_eq!(scalar("a\nb"), "\"a\\nb\"", "inline scalars never use a literal block");
}

#[test]
fn nested_blocks_use_four_space_indent() {
    let text = emit(&[
        ("workspace_id".into(), s("kiki")),
        (
            "repos".into(),
            Value::Seq(vec![
                Value::Map(vec![
                    ("kind".into(), s("backend")),
                    ("services".into(), Value::Seq(vec![s("api"), s("worker")])),
                ]),
                Value::Map(vec![("kind".into(), s("website"))]),
            ]),
        ),
        ("team_folder".into(), Value::Map(vec![("path".into(), s("/tmp/team"))])),
    ]);
    assert_eq!(
        text,
        "workspace_id: kiki\nrepos:\n    - kind: backend\n      services:\n        - api\n        - worker\n    - kind: website\nteam_folder:\n    path: /tmp/team\n"
    );
}

#[test]
fn empty_collections_booleans_and_ints() {
    let text = Mapping::new()
        .set("none", Vec::<Value>::new())
        .set("empty", Value::Map(Vec::new()))
        .set("enabled", true)
        .set("readonly", false)
        .set("loops", 3i64)
        .set("rows", vec![Value::Map(Vec::new()), Value::Seq(Vec::new())])
        .emit();
    assert_eq!(
        text,
        "none: []\nempty: {}\nenabled: true\nreadonly: false\nloops: 3\nrows:\n    - {}\n    - []\n"
    );
}

#[test]
fn multi_line_text_is_a_literal_block() {
    let text = Mapping::new().set("prompt", "one\ntwo\n").set("next", "x").emit();
    assert_eq!(text, "prompt: |\n    one\n    two\nnext: x\n");
}

#[test]
fn keys_are_quoted_like_values() {
    assert_eq!(
        Mapping::new()
            .set("a b", "x")
            .set("true", "yes!")
            .set("k: v", "z")
            .emit(),
        "a b: x\n\"true\": yes!\n'k: v': z\n"
    );
}

#[test]
fn nested_sequences_start_inline() {
    let text = emit(&[(
        "m".into(),
        Value::Seq(vec![Value::Seq(vec![s("a"), s("b")]), Value::Seq(vec![s("c")])]),
    )]);
    assert_eq!(text, "m:\n    - - a\n      - b\n    - - c\n");
}

#[test]
fn builder_helpers_skip_empty_entries() {
    let map = Mapping::new()
        .set_unless_empty("a", "")
        .set_unless_empty("b", "x")
        .set_if(false, "c", true)
        .set_if(true, "d", true);
    assert!(!map.is_empty());
    assert_eq!(map.emit(), "b: x\nd: true\n");
    assert!(Mapping::new().is_empty());
    assert_eq!(Mapping::new().emit(), "");
}

#[test]
fn emitted_text_reads_back() {
    let text = Mapping::new()
        .set("id", "12")
        .set("flag", true)
        .set("note", "it's: odd")
        .emit();
    let root = crate::read::parse(&text).unwrap().unwrap();
    let fields = crate::read::Fields::of(&root).unwrap();
    assert_eq!(fields.string("id").unwrap(), "12");
    assert!(fields.bool("flag").unwrap());
    assert_eq!(fields.string("note").unwrap(), "it's: odd");
}

#[test]
fn unprintable_and_ambiguous_text_is_quoted() {
    // Characters above U+FFFF and U+0085 are not printable for libyaml, so they are escaped.
    assert_eq!(scalar("😀"), "\"\\U0001F600\"");
    assert_eq!(scalar("\u{85}"), "\"\\N\"");
    // Sexagesimal-looking numbers and old booleans would read back as other types.
    assert_eq!(scalar("1:2:3"), "\"1:2:3\"");
    assert_eq!(scalar("OFF"), "\"OFF\"");
    // Dates read back as timestamps.
    assert_eq!(scalar("2020-01-02"), "\"2020-01-02\"");
    assert_eq!(scalar("2020-13-45"), "2020-13-45");
}

#[test]
fn block_literal_chomping_matches_yaml_v3() {
    assert_eq!(Mapping::new().set("a", "x\ny").emit(), "a: |-\n    x\n    y\n");
    assert_eq!(Mapping::new().set("a", "x\ny\n").emit(), "a: |\n    x\n    y\n");
    assert_eq!(Mapping::new().set("a", "x\n\n").emit(), "a: |+\n    x\n\n");
}
