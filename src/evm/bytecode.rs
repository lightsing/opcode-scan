//! EVM byte code generator

use crate::evm::opcode::OpcodeId;
use cbor::Decoder as CborDecoder;

/// Helper struct that represents a single element in a bytecode.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct BytecodeElement {
    /// The byte value of the element.
    pub value: u8,
    /// Whether the element is an opcode or push data byte.
    pub is_code: bool,
}

/// EVM Bytecode
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Bytecode {
    /// Vector for bytecode elements.
    pub code: Vec<BytecodeElement>,
    num_opcodes: usize,
}

impl Bytecode {
    /// Write op
    pub fn write_op(&mut self, op: OpcodeId) -> &mut Self {
        self.write_op_internal(op.as_u8())
    }

    fn write_op_internal(&mut self, op: u8) -> &mut Self {
        self.num_opcodes += 1;
        self.write(op, true)
    }

    /// Write byte
    pub fn write(&mut self, value: u8, is_code: bool) -> &mut Self {
        self.code.push(BytecodeElement { value, is_code });
        self
    }
}

impl From<Vec<u8>> for Bytecode {
    fn from(mut input: Vec<u8>) -> Self {
        trim_metadata(&mut input);
        let mut code = Bytecode::default();

        let mut input_iter = input.iter();
        while let Some(byte) = input_iter.next() {
            let op = OpcodeId::from(*byte);
            code.write_op(op);
            if op.is_push_with_data() {
                let n = op.postfix().expect("opcode with postfix");
                for _ in 0..n {
                    match input_iter.next() {
                        Some(v) => {
                            code.write(*v, false);
                        }
                        None => {
                            // out of boundary is allowed
                            // see also: https://github.com/ethereum/go-ethereum/blob/997f1c4f0abcd78f645e6e7ced6db4b42ad59c9d/core/vm/analysis.go#L65
                            break;
                        }
                    }
                }
            }
        }

        code
    }
}

fn trim_metadata(bytecode: &mut Vec<u8>) {
    if bytecode.len() <= 2 {
        return;
    }
    // cbor length is last 2 bytes of bytecode, u16 big endian
    let cbor_length =
        u16::from_be_bytes([bytecode[bytecode.len() - 2], bytecode[bytecode.len() - 1]]) as usize;
    // if bytecode length is less than cbor length, it's not a valid cbor
    if bytecode.len() - 2 < cbor_length {
        return;
    }
    let mut decode =
        CborDecoder::from_bytes(&bytecode[bytecode.len() - 2 - cbor_length..bytecode.len() - 2]);
    let mut items = decode.items();
    while let Some(item) = items.next() {
        if let Err(_) = item {
            return;
        }
    }
    bytecode.truncate(bytecode.len() - 2 - cbor_length);
}
