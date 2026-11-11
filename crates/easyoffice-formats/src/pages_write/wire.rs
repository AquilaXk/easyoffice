//! Protobuf wire encoding of the Pages writer.
//!
//! Messages are built as small trees ([`Fld`]) instead of byte strings so that a reference to
//! another object is a typed leaf ([`Val::Ref`]), not something a later pass has to guess from
//! bytes. [`encode`] writes the fields in the order given; [`is_ascending`] states the rule the
//! writer keeps (every message lists its fields in ascending field-number order, as the apps do).

/// One field value.
pub(super) enum Val {
    Varint(u64),
    /// 32-bit float (`fixed32`).
    F32(f32),
    /// Length-delimited bytes (strings, packed numbers).
    Bytes(Vec<u8>),
    Msg(Vec<Fld>),
    /// `TSP.Reference`: the message `{1: identifier}`.
    Ref(u64),
}

/// One field: number and value.
pub(super) struct Fld {
    pub number: u32,
    pub val: Val,
}

pub(super) fn varint(number: u32, value: u64) -> Fld {
    Fld {
        number,
        val: Val::Varint(value),
    }
}

pub(super) fn float(number: u32, value: f32) -> Fld {
    Fld {
        number,
        val: Val::F32(value),
    }
}

pub(super) fn string(number: u32, text: &str) -> Fld {
    bytes(number, text.as_bytes().to_vec())
}

pub(super) fn bytes(number: u32, value: Vec<u8>) -> Fld {
    Fld {
        number,
        val: Val::Bytes(value),
    }
}

pub(super) fn msg(number: u32, fields: Vec<Fld>) -> Fld {
    Fld {
        number,
        val: Val::Msg(fields),
    }
}

pub(super) fn reference(number: u32, target: u64) -> Fld {
    Fld {
        number,
        val: Val::Ref(target),
    }
}

/// Packed varints, as `MessageInfo.version` and `object_references` use.
pub(super) fn packed(number: u32, values: &[u64]) -> Fld {
    let mut out = Vec::new();
    for value in values {
        write_varint(&mut out, *value);
    }
    bytes(number, out)
}

pub(super) fn write_varint(out: &mut Vec<u8>, mut value: u64) {
    while value >= 0x80 {
        out.push((value & 0x7F) as u8 | 0x80);
        value >>= 7;
    }
    out.push(value as u8);
}

const WIRE_VARINT: u64 = 0;
const WIRE_FIXED32: u64 = 5;
const WIRE_LEN: u64 = 2;

/// Encodes `fields` in the order given.
pub(super) fn encode(fields: &[Fld]) -> Vec<u8> {
    let mut out = Vec::new();
    encode_into(fields, &mut out);
    out
}

fn encode_into(fields: &[Fld], out: &mut Vec<u8>) {
    for field in fields {
        let key = u64::from(field.number) << 3;
        match &field.val {
            Val::Varint(v) => {
                write_varint(out, key | WIRE_VARINT);
                write_varint(out, *v);
            }
            Val::F32(v) => {
                write_varint(out, key | WIRE_FIXED32);
                out.extend_from_slice(&v.to_le_bytes());
            }
            Val::Bytes(b) => {
                write_varint(out, key | WIRE_LEN);
                write_varint(out, b.len() as u64);
                out.extend_from_slice(b);
            }
            Val::Msg(inner) => {
                write_varint(out, key | WIRE_LEN);
                let body = encode(inner);
                write_varint(out, body.len() as u64);
                out.extend_from_slice(&body);
            }
            Val::Ref(id) => {
                write_varint(out, key | WIRE_LEN);
                let mut body = Vec::with_capacity(11);
                write_varint(&mut body, 1 << 3 | WIRE_VARINT);
                write_varint(&mut body, *id);
                write_varint(out, body.len() as u64);
                out.extend_from_slice(&body);
            }
        }
    }
}

/// True when every message of the tree lists its fields in non-decreasing number order.
pub(super) fn is_ascending(fields: &[Fld]) -> bool {
    fields.windows(2).all(|w| w[0].number <= w[1].number)
        && fields.iter().all(|f| match &f.val {
            Val::Msg(inner) => is_ascending(inner),
            _ => true,
        })
}

/// Every reference leaf of the tree with the field path that leads to it (the path ends with the
/// number of the field that holds the reference).
pub(super) fn references(fields: &[Fld]) -> Vec<(Vec<u32>, u64)> {
    let mut out = Vec::new();
    collect(fields, &mut Vec::new(), &mut out);
    out
}

fn collect(fields: &[Fld], path: &mut Vec<u32>, out: &mut Vec<(Vec<u32>, u64)>) {
    for field in fields {
        path.push(field.number);
        match &field.val {
            Val::Ref(id) => out.push((path.clone(), *id)),
            Val::Msg(inner) => collect(inner, path, out),
            _ => {}
        }
        path.pop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reference_is_a_message_with_one_varint() {
        assert_eq!(encode(&[reference(2, 300)]), [0x12, 0x03, 0x08, 0xAC, 0x02]);
    }

    #[test]
    fn nested_order_is_checked() {
        assert!(is_ascending(&[
            varint(1, 0),
            msg(2, vec![varint(1, 1), varint(1, 2)])
        ]));
        assert!(!is_ascending(&[varint(2, 0), varint(1, 0)]));
        assert!(!is_ascending(&[msg(1, vec![varint(3, 0), varint(2, 0)])]));
    }

    #[test]
    fn reference_paths_end_at_the_holding_field() {
        let tree = [
            msg(11, vec![msg(7, vec![reference(1, 9)])]),
            reference(2, 5),
        ];
        assert_eq!(references(&tree), [(vec![11, 7, 1], 9), (vec![2], 5)]);
    }

    #[test]
    fn float_and_packed_encode() {
        assert_eq!(encode(&[float(3, 1.0)]), [0x1D, 0, 0, 0x80, 0x3F]);
        assert_eq!(encode(&[packed(2, &[1, 0, 5])]), [0x12, 3, 1, 0, 5]);
    }
}
