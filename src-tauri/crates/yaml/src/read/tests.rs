use super::*;

fn fields_of(text: &str) -> Node {
    parse(text).unwrap().unwrap()
}

#[test]
fn empty_text_has_no_document() {
    assert!(parse("").unwrap().is_none());
    assert!(parse("# only a comment\n").unwrap().is_none());
}

#[test]
fn plain_numbers_read_as_text_in_string_fields() {
    let root = fields_of("board_id: 12\nname: 'quoted'\nflag: yes\nnothing:\n");
    let fields = Fields::of(&root).unwrap();
    assert_eq!(fields.string("board_id").unwrap(), "12");
    assert_eq!(fields.string("name").unwrap(), "quoted");
    assert_eq!(fields.string("nothing").unwrap(), "");
    assert_eq!(fields.string("missing").unwrap(), "");
    assert!(fields.bool("flag").unwrap(), "yaml.v3 reads yes into a bool");
}

#[test]
fn bool_and_int_decoding_follow_yaml_v3() {
    let root = fields_of("a: true\nb: 'true'\nc: 7\nd: 0x10\ne: 1.5\nf: text\n");
    let fields = Fields::of(&root).unwrap();
    assert!(fields.bool("a").unwrap());
    assert!(
        fields.bool("b").is_err(),
        "a quoted string that is not a bool spelling cannot decode into bool"
    );
    assert_eq!(fields.int("c").unwrap(), 7);
    assert_eq!(fields.int("d").unwrap(), 16);
    assert_eq!(fields.int("e").unwrap(), 1);
    assert!(fields.int("f").is_err());
}

#[test]
fn repeated_keys_and_non_mappings_are_errors() {
    let dup = fields_of("a: 1\na: 2\n");
    assert_eq!(Fields::of(&dup).err().unwrap(), "mapping key \"a\" already defined");
    let seq = fields_of("- a\n");
    assert_eq!(Fields::of(&seq).err().unwrap(), "cannot unmarshal !!seq into a struct");
    let null = fields_of("~\n");
    assert!(Fields::of(&null).is_ok(), "null is an empty struct");
}

#[test]
fn anchors_and_aliases_are_expanded() {
    let root = fields_of("a: &x hello\nb: *x\n");
    let fields = Fields::of(&root).unwrap();
    assert_eq!(fields.string("b").unwrap(), "hello");
}
