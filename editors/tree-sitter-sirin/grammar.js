/**
 * Tree-sitter grammar for Sirin.
 *
 * Scope: syntax highlighting, not semantic analysis — that is the LSP's job
 * (sirin-lsp). So this is deliberately a token-stream grammar: a flat repeat of
 * lexical tokens plus a handful of two-token contextual shapes (`fn NAME`,
 * `class NAME`, `NAME(`, `.NAME`) that highlighting actually needs. Keeping it
 * flat means malformed or half-typed code never produces an ERROR node that
 * kills highlighting for the rest of the buffer.
 */

const KEYWORDS = [
  'return', 'if', 'else', 'while', 'for', 'in', 'break', 'continue',
  'enum', 'match', 'and', 'or', 'try',
  'class', 'abstract', 'extends', 'implements', 'interface', 'impl',
  'is', 'init', 'default', 'mut', 'use', 'type',
  'fn', 'async', 'spawn', 'await',
];

const PRIMITIVE_TYPES = [
  'int', 'bool', 'float', 'str',
  'u8', 'u16', 'u32', 'u64', 'i8', 'i16', 'i32', 'i64',
];

const COLLECTION_TYPES = ['Array', 'Vec', 'Map', 'Set'];

// Option / Result constructors: `Some(x)`, `None`, `Ok(v)`, `Err(msg)`.
const CONSTRUCTORS = ['Some', 'None', 'Ok', 'Err'];

const OPERATORS = [
  '?=', ':=', '==', '!=', '>=', '<=', '->', '=>', '::', '..',
  '=', '>', '<', '+', '-', '*', '/', '!', '?',
];

module.exports = grammar({
  name: 'sirin',

  word: $ => $.identifier,

  extras: $ => [/[ \t\r\n]/, $.line_comment, $.block_comment],

  rules: {
    source_file: $ => repeat($._item),

    _item: $ => choice(
      $.block,
      $.definition,
      $.call,
      $.field,
      $.keyword,
      $.self_keyword,
      $.primitive_type,
      $.collection_type,
      $.constructor,
      $.boolean,
      $.float,
      $.integer,
      $.string,
      $.identifier,
      $.operator,
      $.bracket,
      $.delimiter,
    ),

    // The one nested rule: enough structure for auto-indent, cheap enough that a
    // half-typed brace only breaks the block being edited.
    block: $ => seq('{', repeat($._item), '}'),

    // `fn soma`, `class Animal`, `enum Color`, `interface Shape`, `impl X`, `use mod`
    // prec(1): `fn` followed by a name is a definition, not a bare keyword.
    definition: $ => prec(1, seq(
      field('kind', alias(
        choice('fn', 'class', 'enum', 'interface', 'impl', 'abstract', 'use', 'type'),
        $.keyword,
      )),
      field('name', $.identifier),
    )),

    // `foo(` — the callee, not the whole call expression.
    call: $ => prec(1, seq(field('name', $.identifier), '(')),

    // `.push`, `.nome`
    field: $ => prec(1, seq('.', field('name', $.identifier))),

    keyword: _ => choice(...KEYWORDS),
    self_keyword: _ => 'self',
    primitive_type: _ => choice(...PRIMITIVE_TYPES),
    collection_type: _ => choice(...COLLECTION_TYPES),
    constructor: _ => choice(...CONSTRUCTORS),

    boolean: _ => choice('true', 'false'),

    identifier: _ => /[a-zA-Z_][a-zA-Z0-9_]*/,

    integer: _ => /[0-9]+/,
    // No leading-dot form (`.5`): it collides with the `..` range operator.
    float: _ => /[0-9]+\.[0-9]+([eE][+-]?[0-9]+)?/,
    string: $ => seq(
      '"',
      repeat(choice(
        token.immediate(prec(1, /[^"\\]+/)),
        $.escape_sequence,
      )),
      '"',
    ),
    escape_sequence: _ => token.immediate(/\\./),

    operator: _ => choice(...OPERATORS),
    bracket: _ => choice('(', ')', '[', ']'),
    delimiter: _ => choice(',', ':', '.'),

    line_comment: _ => token(seq('//', /[^\n]*/)),
    block_comment: _ => token(seq('/*', /[^*]*\*+([^/*][^*]*\*+)*/, '/')),
  },
});
