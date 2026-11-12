//! Table des opcodes de la machine virtuelle des scripts Fallout 2 (avec les ajouts de sfall).

use crate::opcode_table::OPCODES;

pub const O_STRINGOP: u16 = 0x9001;
pub const O_FLOATOP: u16 = 0xA001;
pub const O_INTOP: u16 = 0xC001;
/// Premier code invalide parmi les opcodes « simples » (les trois précédents portent un argument).
pub const O_END_OP: u16 = 0x8282;

pub const O_CALL: u16 = 0x8005;
pub const O_JMP: u16 = 0x8004;
pub const O_POP: u16 = 0x801A;
pub const O_POP_RETURN: u16 = 0x801C;
pub const O_FETCH_GLOBAL: u16 = 0x8012;
pub const O_STORE_GLOBAL: u16 = 0x8013;
pub const O_FETCH_EXTERNAL: u16 = 0x8014;
pub const O_STORE_EXTERNAL: u16 = 0x8015;
pub const O_IF: u16 = 0x802F;
pub const O_WHILE: u16 = 0x8030;
pub const O_STORE: u16 = 0x8031;
pub const O_FETCH: u16 = 0x8032;

/// Comment l'opcode s'écrit dans une source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Une ligne à lui seul : `critter_heal(obj, 5);`
    Statement,
    /// Une valeur : `obj_name(who)`.
    Expression,
    /// Un opérateur entre ses deux opérandes : `a + b`.
    Infix,
    /// Une valeur, ou une ligne si rien ne l'utilise.
    ExprStatement,
}

#[derive(Debug, Clone, Copy)]
pub struct OpcodeInfo {
    pub code: u16,
    pub name: &'static str,
    /// Nombre de valeurs dépilées.
    pub argc: u8,
    pub kind: Kind,
}

pub fn lookup(code: u16) -> Option<&'static OpcodeInfo> {
    OPCODES
        .binary_search_by_key(&code, |o| o.code)
        .ok()
        .map(|i| &OPCODES[i])
}

/// Un opcode porte-t-il un argument de 4 octets ?
pub fn has_argument(code: u16) -> bool {
    matches!(code, O_STRINGOP | O_FLOATOP | O_INTOP)
}

/// Un code est-il valide dans le corps d'une procédure ?
pub fn is_valid(code: u16) -> bool {
    has_argument(code) || (0x8000..O_END_OP).contains(&code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_is_sorted_and_known() {
        assert!(OPCODES.windows(2).all(|w| w[0].code < w[1].code));
        assert_eq!(lookup(O_IF).unwrap().name, "if");
        assert!(lookup(0x1234).is_none());
    }
}
