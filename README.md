# Sirin

Sirin is a small, statically typed programming language that compiles to
native executables through C. It aims for the feel of a scripting language,
with no semicolons, type inference and top-level code, while giving you
ownership, no `null`, no exceptions and a single self-contained binary at the
end.

```sirin
use sirin.io

fn find(id: int) -> str? {
    if (id == 1) { return Some("Julius") }
    return None
}

fn main() {
    if Some(name) = find(1) {
        println(name)
    } else {
        println("nobody")
    }
}
```

```
$ sirin run hello.sn
Julius
```

> **Status:** early development (v0.1). The language and the standard library
> will change, and some features are only partly implemented. See
> [Status](#status).

## Why Sirin

- **Native and small.** Sirin compiles to C and then to a native executable.
  There is no VM, no garbage collector and no runtime to install.
- **No null, no exceptions.** A value that may be missing has type `T?`
  (`Option`). An operation that may fail returns `T!` (`Try`). The compiler
  makes you handle both.
- **Ownership without lifetimes.** Assigning a value moves it, and the
  checker reports any use after the move. You copy on purpose with `:=` or
  `::clone`. There are no lifetime annotations and no borrow syntax to learn.
- **Error messages meant for people.** Diagnostics point at the exact source
  span, explain what went wrong and usually suggest a fix.
- **One toolchain.** A single `sirin` binary type-checks, builds and runs
  programs. On Windows it includes its own C compiler (TinyCC), so nothing else
  is needed.
- **Editor support from day one.** A language server (`sirin-lsp`) and a Zed
  extension with a tree-sitter grammar.

## A tour of the language

### Variables and types

Types are inferred. Add an annotation when you want a specific one.

```sirin
name = "Sirin"      // str
x = 42              // int
ratio = 0.5         // float
ok = true           // bool
v: u8 = 255         // explicit type
```

Primitive types: `int`, `float`, `bool`, `str`, `u8` `u16` `u32` `u64`,
`i8` `i16` `i32` `i64`.

Statements can go at the top level, so a script needs no `main`. A file may
also define `fn main()`.

### Functions

```sirin
fn add(a: int, b: int) -> int {
    return a + b
}

fn greet(name: str) -> str => "hello, " + name   // single-expression body

fn clamp(n: int) -> int {
    return 0 if n < 0                             // conditional return
    return n
}
```

### Control flow

Conditions go in parentheses.

```sirin
if (x > 10) {
    println("big")
} else {
    println("small")
}

n = 0
while (n < 3) {
    n = n + 1
}

for i in 0..3 {      // half-open range: 0, 1, 2
    println(i)
}
```

`break` and `continue` work in both loops.

The compiler rewrites self-recursive tail calls as loops, so recursion does
not grow the stack:

```sirin
fn fact(n: int, acc: int) -> int {
    return acc if n == 0
    return fact(n - 1, acc * n)
}
```

### Enums and match

Enums are sum types, and each variant can carry values. `match` must cover
every variant, or end with a `_` arm.

```sirin
enum Shape {
    Circle(float),
    Rect(float, float),
    Point
}

fn area(s: Shape) -> float {
    match s {
        Circle(r) => { return 3.14 * r * r },
        Rect(w, h) => { return w * h },
        Point => { return 0.0 }
    }
}

println(area(Shape.Circle(2.0)))
println(area(Shape.Point))
```

### Ownership: move, copy and clone

Assignment moves a value, and the old name can't be used afterwards:

```sirin
a = "hi"
b = a
println(a)   // error: use of moved value
```

```
Error: use of moved value
    ╭─[ tour.sn:3:9 ]
    │
  3 │ println(a)
    │         ┬
    │         ╰── `a` was moved into `b` and can no longer be used
    │
    │ Help: clone it where it is moved (`a::clone`) if you still need `a` afterwards
────╯
```

To keep both, copy with `:=` or clone at the call site:

```sirin
a = "hi"
b := a                 // b is a copy, a is still usable
clients.push(conn::clone)
```

### Option and Try instead of null and exceptions

`T?` means "a `T` or nothing", and `T!` means "a `T` or an error".
`Option[T]` and `Try[T]` are the long forms.

```sirin
fn parse(s: str) -> int! {
    if (s.len() == 0) {
        return Err("empty")
    }
    return Ok(s.to_int())
}

fn sum(a: str, b: str) -> int! {
    x ?= parse(a)          // unwrap Ok, or return the Err to the caller
    y ?= parse(b)
    return Ok(x + y)
}

if Ok(v) = sum("2", "3") {
    println(v)
}
if Err(e) = sum("", "3") {
    println(e)             // "empty"
}
```

Helper methods: `unwrap()`, `unwrap_or(default)`, `is_some()`, `is_none()`,
`is_ok()` and `is_err()`.

### Classes, interfaces and inheritance

```sirin
interface Barker {
    fn bark() -> str
}

class Animal {
    name: str
    mut count: u8

    default {                 // runs for Animal()
        count = 0
    }

    init(n: str) {            // runs for Animal("Rex")
        name = n
        count = 0
    }

    fn describe() -> str => name
}

class Dog extends Animal implements Barker {
    breed: str

    init(n: str, b: str) {
        name = n
        breed = b
    }

    fn bark() -> str => self.name
}

a = Animal("Rex")               // init
b = Animal()                    // default
c = Animal { name: "Fido" }     // field initializer
d = Dog("Bolt", "Labrador")
```

A class that is missing an interface method is a compile error. Classes can
also be `abstract`.

### Structural objects

Anonymous objects are typed by their shape, so a function can accept any
value with the right fields. `type` gives a shape a name.

```sirin
type User = { name: str, age: int }

fn age_of(u: User) -> int {
    return u.age
}

user = { name: "Julius", age: 24 }
println(age_of(user))

// JSON straight into a typed object
body = "{\"name\": \"Aria\", \"age\": 30}"
other: User = body.to_object()
```

### Collections and strings

```sirin
arr: Array[int] = [1, 2, 3]
v: Vec[u8] = Vec(10)
v.push(42)

m: Map[str, int] = Map()
m.insert("age", 25)

s: Set[int] = Set()
s.insert(1)
has = s.contains(1)
```

Strings come with `len`, `to_upper`, `to_lower`, `slice`, `index_of`,
`contains`, `starts_with`, `ends_with`, `replace`, `trim`, `split` and
`to_int`.

`impl` adds methods to existing types, including the built-in ones:

```sirin
impl str {
    fn shout() -> str => self.to_upper()
}

println("hi".shout())   // HI
```

### Async, channels and networking

`async fn`, `spawn` and `.await` are built in. So are typed channels and TCP
sockets.

```sirin
use sirin.io
use sirin.async
use sirin.net

async fn handle(conn: TcpStream) {
    msg = conn.read().await
    conn.write(msg)          // echo
    conn.close()
}

async fn accept_loop(l: TcpListener) {
    conn = l.accept().await
    spawn handle(conn)
    return accept_loop(l)    // tail call, runs as a loop
}

listener = TcpListener("0.0.0.0", 3000)
spawn accept_loop(listener)
```

`examples/http` contains a small HTTP server written entirely in Sirin, and
`examples/chat` has a broadcast chat server and client.

### Modules

`use sirin.io`, `use sirin.async` and `use sirin.net` import the standard
library. `use http` imports `http.sn` from the same directory, and a module
can import others (`use types`, `use transport`). Names that start with `_`
are private to their module. The compiler reports circular imports and
missing modules.

## Installation

### Prebuilt binaries

Download the archive for your platform from the
[latest release](https://github.com/KingTimer12/sirin/releases/latest):

| Platform              | Archive                                |
| --------------------- | -------------------------------------- |
| Linux x86_64          | `sirin-x86_64-unknown-linux-gnu.tar.gz` |
| macOS Apple Silicon   | `sirin-aarch64-apple-darwin.tar.gz`     |
| macOS Intel           | `sirin-x86_64-apple-darwin.tar.gz`      |
| Windows x86_64        | `sirin-x86_64-pc-windows-msvc.zip`      |

Each archive contains `sirin` (the compiler) and `sirin-lsp` (the language
server). Put both on your `PATH`. The Windows archive also contains a `tcc/`
folder, which must stay next to `sirin.exe`.

On Linux and macOS, `sirin build` uses the system C compiler (`cc` or
`clang`), which must be installed.

### From source

You need a recent stable Rust toolchain (edition 2024).

```sh
git clone --recursive https://github.com/KingTimer12/sirin
cd sirin
cargo install --path sirin-cli
cargo install --path sirin-lsp
```

`--recursive` also fetches the TinyCC submodule. In a clone made without it,
run `git submodule update --init --recursive`.

## Usage

```
sirin check  <file>          Type-check a file without building it
sirin build  <file>          Compile a file into an executable
sirin run    <file>          Compile a file and run it
sirin run    <file> --watch  Rebuild and restart on every save
sirin emit-c <file>          Write the C source and the runtime it needs
sirin tokens <file>          Print the tokens (debugging)
sirin ast    <file>          Print the syntax tree (debugging)
```

Source files use the `.sn` extension. `examples/` has a program for most
features.

## Editor support

- **Zed:** the extension in [`editors/zed`](editors/zed) adds syntax
  highlighting (tree-sitter) and language server features. On first use it
  downloads `sirin-lsp` from the latest GitHub release. If a `sirin-lsp` is on
  your `PATH`, the extension uses that one instead. To install it, open Zed,
  run `zed: install dev extension` and choose `editors/zed`.
- **Other editors:** `sirin-lsp` is a standard LSP server over stdio. It
  reports syntax and type errors as you type.
- **Grammar:** [`editors/tree-sitter-sirin`](editors/tree-sitter-sirin)
  works with any editor that supports tree-sitter.

## How it works

```
 .sn source ─▶ lexer ─▶ parser ─▶ type checker ─▶ C codegen ─▶ C compiler ─▶ executable
             (logos) (chumsky)  (types, moves)               (TinyCC / cc)
```

| Crate               | Role                                                        |
| ------------------- | ----------------------------------------------------------- |
| `sirin-cli`         | The `sirin` command and the module resolver                  |
| `sirin-lexer`       | Tokenizer built on `logos`                                   |
| `sirin-parser`      | Parser built on `chumsky`, producing the AST                 |
| `sirin-typechecker` | Type inference, ownership/move checking, interface checks    |
| `sirin-codegen-c`   | C emitter, tail-call optimization, embedded TinyCC           |
| `sirin-diagnostics` | Shared diagnostics, rendered with `ariadne`                  |
| `sirin-lsp`         | Language server built on `tower-lsp`                         |
| `sirin-runtime`     | C runtime linked into every program (see below)              |

The runtime is split by area, and a build only compiles the parts the
program uses:

| Directory                    | Contents                                               |
| ---------------------------- | ------------------------------------------------------ |
| `sirin-runtime/core`         | memory, strings, JSON fields, console input            |
| `sirin-runtime/collections`  | `Vec`, `Array`, `Set`, `Map`                           |
| `sirin-runtime/async`        | coroutine scheduler, channels, per-platform contexts   |
| `sirin-runtime/net`          | TCP, UDP, per-platform socket layer                    |

## Status

Sirin is a young project. Known gaps:

- There are no generics for user-defined types yet. The built-in
  collections (`Vec[T]`, `Map[K, V]`, ...) are generic.
- The async scheduler polls instead of sleeping, so a program waiting on a
  socket or on input keeps one CPU core busy.

## Contributing

```sh
cargo build --workspace
cargo test --workspace
```

CI builds the workspace, runs the tests, checks that the generated
tree-sitter parser is up to date, and builds the Zed extension. After
changing `editors/tree-sitter-sirin/grammar.js`, run `tree-sitter generate`
in that directory and commit the result. Then update `rev` in
`editors/zed/extension.toml` to point at that commit.

Releases are built by `.github/workflows/release.yml` when a `v*` tag is
pushed.
