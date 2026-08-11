; Order matters: tree-sitter takes the first matching pattern, so the
; `#eq?`-refined definition rules must precede the generic one.

((definition
   kind: (keyword) @keyword
   name: (identifier) @function)
 (#eq? @keyword "fn"))

((definition
   kind: (keyword) @keyword
   name: (identifier) @namespace)
 (#eq? @keyword "use"))

; `class Animal`, `enum Color`, `interface Shape`, `impl X`, `type Alias`
(definition
  kind: (keyword) @keyword
  name: (identifier) @type)

(call name: (identifier) @function)
(field name: (identifier) @property)

(keyword) @keyword
(self_keyword) @variable.special

(primitive_type) @type.builtin
(collection_type) @type.builtin
(constructor) @constructor

(boolean) @boolean
(integer) @number
(float) @number
(string) @string
(escape_sequence) @string.escape

(identifier) @variable

(operator) @operator
(delimiter) @punctuation.delimiter
(bracket) @punctuation.bracket

(line_comment) @comment
(block_comment) @comment
