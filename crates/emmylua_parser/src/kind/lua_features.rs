#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u64)]
pub enum LuaFeatures {
    // std lua
    Goto = 1,             // "goto"
    BitwiseOperation,     // "5 & 2"
    IntegerFloorDivision, // "5 // 2"
    LocalAttrib,          // "local a<const> = 1"
    GlobalDeclaration,    // "global a = 1"
    NamedVararg,          // "function f(a, b, c, ...args) end"

    // non-standard symbols
    DoubleSlash, // "//"
    SlashStar,   // "/**/"

    // luajit
    ComplexNumber, // "0x1.2p3i"
    LLInteger,     // "0LL"
    BinaryInteger, // "0b1010"

    // luajit2-extension
    PlusAssign,             // "+="
    MinusAssign,            // "-="
    StarAssign,             // "*="
    SlashAssign,            // "/="
    PercentAssign,          // "%="
    CaretAssign,            // "^="
    DoubleSlashAssign,      // "//="
    PipeAssign,             // "|="
    AmpAssign,              // "&="
    ShiftLeftAssign,        // "<<="
    ShiftRightAssign,       // ">>="
    ShrArithmeticAssign,    // "~>>="
    ConcatAssign,           // "..="
    XorAssign,              // "~="
    DoublePipeOr,           // "||"
    DoubleAmpAnd,           // "&&"
    Exclamation,            // "!"
    NotEqual,               // "!="
    Continue,               // "continue"
    ShiftRightArithmetic,   // "~>>"
    Ternary,                // "a ? b : c"
    SafeNavigationOperator, // "?."
    NilCoalescingOperator,  // "??"
    ConstDeclaration,       // "const"
    UnderscoreNumber,       // "1_23"
    ShortFunction,          // | a, b | -> a + b

    // luajit3
    StringInterpolation, // "`"
                         // luajit not consider this syntax
                         // NilCoalescingAssign, // "??="
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LuaFeaturesSet(u64);

impl LuaFeaturesSet {
    pub fn new(features: Vec<LuaFeatures>) -> Self {
        let mut set = LuaFeaturesSet(0);
        for feature in features {
            set.add(feature);
        }
        set
    }

    pub fn features_lua51() -> Self {
        LuaFeaturesSet::default()
    }

    pub fn features_lua52() -> Self {
        let mut set = LuaFeaturesSet::features_lua51();
        set.add(LuaFeatures::Goto);
        set
    }

    pub fn features_lua53() -> Self {
        let mut set = LuaFeaturesSet::features_lua52();
        set.add(LuaFeatures::BitwiseOperation);
        set.add(LuaFeatures::IntegerFloorDivision);
        set
    }

    pub fn features_lua54() -> Self {
        let mut set = LuaFeaturesSet::features_lua53();
        set.add(LuaFeatures::LocalAttrib);
        set
    }

    pub fn features_lua55() -> Self {
        let mut set = LuaFeaturesSet::features_lua54();
        set.add(LuaFeatures::GlobalDeclaration);
        set.add(LuaFeatures::NamedVararg);
        set
    }

    pub fn features_luajit_legacy() -> Self {
        let mut set = LuaFeaturesSet::features_lua51();
        set.add(LuaFeatures::ComplexNumber);
        set.add(LuaFeatures::LLInteger);
        set.add(LuaFeatures::BinaryInteger);
        set.add(LuaFeatures::Goto);
        set
    }

    pub fn features_luajit() -> Self {
        let mut set = LuaFeaturesSet::features_luajit_legacy();

        // luajit-extension
        set.add(LuaFeatures::BitwiseOperation);
        set.add(LuaFeatures::PlusAssign);
        set.add(LuaFeatures::MinusAssign);
        set.add(LuaFeatures::StarAssign);
        set.add(LuaFeatures::SlashAssign);
        set.add(LuaFeatures::PercentAssign);
        set.add(LuaFeatures::CaretAssign);
        set.add(LuaFeatures::PipeAssign);
        set.add(LuaFeatures::AmpAssign);
        set.add(LuaFeatures::ShiftLeftAssign);
        set.add(LuaFeatures::ShiftRightAssign);
        set.add(LuaFeatures::XorAssign);
        set.add(LuaFeatures::DoublePipeOr);
        set.add(LuaFeatures::DoubleAmpAnd);
        set.add(LuaFeatures::Exclamation);
        set.add(LuaFeatures::NotEqual);
        set.add(LuaFeatures::ShiftRightArithmetic);
        set.add(LuaFeatures::ShrArithmeticAssign);
        set.add(LuaFeatures::ConcatAssign);
        set.add(LuaFeatures::Continue);
        set.add(LuaFeatures::Ternary);
        set.add(LuaFeatures::SafeNavigationOperator);
        set.add(LuaFeatures::NilCoalescingOperator);
        set.add(LuaFeatures::ConstDeclaration);
        set.add(LuaFeatures::UnderscoreNumber);
        set.add(LuaFeatures::ShortFunction);
        set
    }

    pub fn features_luajit3() -> Self {
        let mut set = LuaFeaturesSet::features_luajit();

        // luajit3
        set.add(LuaFeatures::IntegerFloorDivision);
        // set.add(LuaFeatures::NilCoalescingAssign);
        set.add(LuaFeatures::StringInterpolation);
        set.add(LuaFeatures::NamedVararg);
        set
    }

    pub fn add(&mut self, symbol: LuaFeatures) {
        self.0 |= 1 << (symbol as u64);
    }

    pub fn extends(&mut self, other: Vec<LuaFeatures>) {
        for symbol in other {
            self.add(symbol);
        }
    }

    pub fn extends_set(&mut self, other: LuaFeaturesSet) {
        self.0 |= other.0;
    }

    pub fn support(&self, symbol: LuaFeatures) -> bool {
        self.0 & (1 << (symbol as u64)) != 0
    }
}
