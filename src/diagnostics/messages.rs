//! Diagnostic message templates (rustc style: lowercase, no trailing period,
//! code in backticks). `%s` placeholders and `%%` are filled by
//! [`super::Diagnostic::formatted`].

// Custom command handler errors
pub const CUSTOM_HANDLER_NOTYPE: &str = "internal compiler error: custom command handler did not assign a return type";
pub const CUSTOM_HANDLER_NOSYMBOL: &str = "internal compiler error: custom command handler did not assign a symbol";

// Code gen internal compiler errors
pub const SYMBOL_IS_NULL: &str = "internal compiler error: no symbol defined for the node";
pub const TYPE_HAS_NO_BASETYPE: &str = "internal compiler error: type `%s` has no base type";
pub const INVALID_CONDITION: &str = "internal compiler error: `%s` is not a supported expression type for conditions";
pub const NULL_CONSTANT: &str = "internal compiler error: `%s` evaluated to a `null` constant value";
pub const EXPRESSION_NO_SUBEXPR: &str = "internal compiler error: no sub expression node";

// AST-node-agnostic messages
pub const GENERIC_INVALID_TYPE: &str = "unknown type `%s`";
/// Args: expected, found.
pub const GENERIC_TYPE_MISMATCH: &str = "mismatched types: expected `%s`, found `%s`";
pub const GENERIC_UNRESOLVED_SYMBOL: &str = "cannot find `%s`";
pub const ARITHMETIC_INVALID_TYPE: &str = "mismatched types: expected `int` or `long`, found `%s`";
pub const INTEGER_VALUE_OUT_OF_RANGE: &str = "integer literal is out of range for type `%s`";

// Script node specific
pub const SCRIPT_REDECLARATION: &str = "`[%s,%s]` is defined multiple times";
pub const SCRIPT_LOCAL_REDECLARATION: &str = "local variable `$%s` is defined multiple times";
pub const SCRIPT_TRIGGER_INVALID: &str = "unknown trigger `%s`";
pub const SCRIPT_COMMAND_ONLY: &str = "a `*` name is only allowed for commands";
pub const SCRIPT_TRIGGER_NO_PARAMETERS: &str = "`%s` scripts cannot have parameters";
pub const SCRIPT_TRIGGER_NO_RETURNS: &str = "`%s` scripts cannot return values";
pub const SCRIPT_SUBJECT_ONLY_GLOBAL: &str = "`%s` scripts only allow the global subject `_`";
pub const SCRIPT_SUBJECT_NO_GLOBAL: &str = "`%s` scripts do not allow the global subject `_`";
pub const SCRIPT_SUBJECT_NO_CATEGORY: &str = "`%s` scripts do not allow category subjects";
pub const SCRIPT_SUBJECT_NO_SPACES: &str = "`%s` scripts do not allow spaces in subjects";

// Switch statement node specific
pub const SWITCH_INVALID_TYPE: &str = "cannot switch on a value of type `%s`";
pub const SWITCH_DUPLICATE_DEFAULT: &str = "duplicate `case default`";
pub const SWITCH_CASE_NOT_CONSTANT: &str = "switch case value is not a constant expression";

// Assignment statement node specific
pub const ASSIGN_MULTI_ARRAY: &str = "arrays are not allowed in multi-assignment statements";

// Expression statement node specific
pub const EXPRESSION_STATEMENT_NO_SIDE_EFFECT: &str = "unused value: this expression has no effect";

// Condition expression specific
pub const CONDITION_INVALID_NODE_TYPE: &str = "a condition must be a comparison or a combination of comparisons";
pub const CONDITION_NOT_VALID: &str = "invalid condition";

// Binary expression specific
pub const BINOP_INVALID_TYPES: &str = "operator `%s` cannot be applied to `%s` and `%s`";
pub const BINOP_TUPLE_TYPE: &str = "%s side of a binary expression must be a single value, found `%s`";

// Call expression specific
pub const COMMAND_REFERENCE_UNRESOLVED: &str = "cannot find command `%s`";
pub const COMMAND_NOARGS_EXPECTED: &str = "command `%s` takes no arguments, found `%s`";
pub const HOOK_REFERENCE_UNRESOLVED: &str = "cannot find clientscript `%s`";
pub const HOOK_NOARGS_EXPECTED: &str = "clientscript `%s` takes no arguments, found `%s`";
pub const HOOK_INVALID_CALL: &str = "a hook names a clientscript: `script` or `script(args)`";
pub const HOOK_NOT_LITERAL: &str = "a hook must be a string literal (without `<...>` interpolation) or `null`";
pub const HOOK_NO_TRIGGERS: &str = "this hook takes no trigger list";
pub const HOOK_TRIGGER_INVALID: &str = "`%s` is not a %s";
pub const PROC_REFERENCE_UNRESOLVED: &str = "cannot find proc `~%s`";
pub const PROC_NOARGS_EXPECTED: &str = "proc `~%s` takes no arguments, found `%s`";
pub const JUMP_REFERENCE_UNRESOLVED: &str = "cannot find label `@%s`";
pub const JUMP_NOARGS_EXPECTED: &str = "label `@%s` takes no arguments, found `%s`";

// Local variable specific
pub const LOCAL_DECLARATION_INVALID_TYPE: &str = "a local variable cannot have type `%s`";
pub const LOCAL_PARAMETER_INVALID_TYPE: &str = "a parameter cannot have type `%s`";
pub const LOCAL_REFERENCE_UNRESOLVED: &str = "cannot find local variable `$%s`";
pub const LOCAL_REFERENCE_NOT_ARRAY: &str = "cannot index `$%s`: it is not an array";
pub const LOCAL_ARRAY_INVALID_TYPE: &str = "`%s` cannot be used as an array element type";
pub const LOCAL_ARRAY_REFERENCE_NOINDEX: &str = "array `$%s` used without an index";

// Game var specific
pub const GAME_REFERENCE_UNRESOLVED: &str = "cannot find game variable `%%%s`";

// Constant variable specific
pub const CONSTANT_REFERENCE_UNRESOLVED: &str = "cannot find constant `^%s`";
pub const CONSTANT_CYCLIC_REF: &str = "cycle detected when evaluating constants: %s";
pub const CONSTANT_UNKNOWN_TYPE: &str = "cannot infer the type of `^%s`";
pub const CONSTANT_PARSE_ERROR: &str = "constant value `%s` is not a valid `%s`";
pub const CONSTANT_NONCONSTANT: &str = "the value of `%s` is not a constant expression";

// Null literal specific
pub const NULL_AMBIGUOUS: &str = "cannot infer the type of `null` here";

// Feature flag specific
pub const FEATURE_DISABLED_TRIGGER: &str = "trigger `%s` is disabled";
pub const FEATURE_DISABLED_COMMAND: &str = "command `%s` is disabled";
pub const FEATURE_DISABLED_TYPE: &str = "type `%s` is disabled";
pub const FEATURE_DISABLED_LOCAL: &str = "local variables are disabled";
pub const FEATURE_DISABLED_BOOLEAN: &str = "booleans are disabled";
pub const FEATURE_DISABLED_OPERATOR: &str = "operator `%s` is disabled";
pub const FEATURE_DISABLED_CALC: &str = "`calc(...)` is disabled";
pub const LOCAL_DECLARATION_NOT_TOPLEVEL: &str = "local variables can only be declared at the top level of a script";

// Pointer checking specific
pub const POINTER_UNINITIALIZED: &str = "use of uninitialized pointer `%s`";
pub const POINTER_CORRUPTED: &str = "use of corrupted pointer `%s`";
pub const POINTER_CORRUPTED_LOC: &str = "`%s` corrupted here";
pub const POINTER_REQUIRED_LOC: &str = "`%s` required here";
