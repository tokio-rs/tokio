extern crate tokio_buf;

use tokio_buf::SizeHint;

#[test]
fn size_hint() {
    let hint = SizeHint::new();
    assert_eq!(hint.lower(), 0);
    assert!(hint.upper().is_none());

    let mut hint = SizeHint::new();
    hint.set_lower(100);
    assert_eq!(hint.lower(), 100);
    assert!(hint.upper().is_none());

    let mut hint = SizeHint::new();
    hint.set_upper(200);
    assert_eq!(hint.lower(), 0);
    assert_eq!(hint.upper(), Some(200));

    let mut hint = SizeHint::new();
    hint.set_lower(100);
    hint.set_upper(100);
    assert_eq!(hint.lower(), 100);
    assert_eq!(hint.upper(), Some(100));
}

#[test]
#[should_panic]
fn size_hint_lower_bigger_than_upper() {
    let mut hint = SizeHint::new();
    hint.set_upper(100);
    hint.set_lower(200);
}

#[test]
#[should_panic]
fn size_hint_upper_less_than_lower() {
    let mut hint = SizeHint::new();
    hint.set_lower(200);
    hint.set_upper(100);
}
