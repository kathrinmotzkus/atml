# Understanding ABNF (in the context of ATML)

This document is a gentle introduction to ABNF — the notation used to *define*
TOML and, on top of it, ATML. It assumes you know TOML from using it, but have
never looked at how a format is formally specified. It has two halves: first,
what ABNF is and why it exists; then, a tour of every ABNF concept that appears
in `grammar/atml.abnf`, each shown with a real rule from that file.

---

## Part 1 — What ABNF is, and why we need it

### From examples to a definition

Most people meet a format the way you met TOML: through **examples**. You see

```
name = "Kathrin"
port = 8080
[server]
```

and you infer the rules. Examples are wonderful for learning, but they have a
blind spot: they only ever show you what *is* allowed. They never show you the
exact edge of the language — what is *not* allowed, and why. Is `port2 = 1`
valid? Is `2port = 1`? May a key be empty? May a number start with a leading
zero? Examples hint at answers; they never state them exhaustively.

A **grammar** is the exhaustive statement behind the examples. It defines,
precisely and completely, which sequences of characters form a valid document
and which do not. ABNF is one standard notation for writing such a grammar.

### What ABNF is

ABNF stands for **Augmented Backus–Naur Form**. It is a small, formal language
for describing the syntax of other text-based languages. It is standardized in
**RFC 5234** (2008), with a small later addition in RFC 7405. The same notation
defines countless internet formats — URLs, email headers, HTTP — and, since
December 2025, TOML v1.1.0.

An ABNF grammar is a list of **rules**. Each rule gives a name to a pattern of
characters. Rules refer to other rules, forming a tree that bottoms out in
literal characters. A document is valid if, and only if, it can be matched
against the grammar's top rule all the way down to those literals.

### Why we need it

A grammar earns its keep for three reasons:

**It removes ambiguity.** A sentence like "a key may contain letters and
numbers" leaves real questions open: which letters (ASCII? Unicode?), may a
digit come first, is an underscore a letter? A grammar answers every such
question mechanically, leaving no room for interpretation. Two people reading
the same grammar reach the same conclusion.

**It is a single source of truth for humans *and* machines.** From one ABNF
file you can generate a parser, build a validator, produce test cases, and
check that an implementation is correct. Everyone who implements the format
agrees on what "valid" means, because they all read the same definition. This
is why TOML is *defined* by `toml.abnf` rather than by prose alone.

**It makes extension safe and provable.** This matters directly for ATML. ATML
is built as a strict *superset* of TOML: every valid TOML document must remain
a valid ATML document. Because both are defined in ABNF, we can extend TOML's
grammar in a way that *provably* only adds to it and never removes — more on
this under the `=/` operator below. Without a formal grammar, "ATML is a
superset of TOML" would be a hope; with one, it is a checkable fact.

### What we use it for in ATML

Concretely, the ATML project uses ABNF to:

- vendor TOML's official grammar unchanged (`grammar/toml-1.1.0.abnf`),
- add ATML's four extensions as purely additive rules (`grammar/atml-ext.abnf`),
- build the combined grammar (`grammar/atml.abnf`) that tools and tests read,
- and drive a test suite that confirms every valid TOML document is valid ATML.

### One honest limit, stated up front

ABNF describes **form**, not **meaning**. It can say that `mode = Strategy::Active`
is shaped like an enum reference. It *cannot* say "the enum `Strategy` must be
declared earlier and must contain `Active`" — that is a rule about relationships
between distant parts of a document, which this kind of grammar cannot express.
Such rules live as prose in `SPEC.md`. Keep this split in mind: the grammar
draws the shapes; the specification text supplies the meaning. (Part 4 returns
to this.)

---

## Part 2 — How to read ABNF: the building blocks

Everything below appears in `grammar/atml.abnf`. Each concept is shown with a
real rule, quoted verbatim, plus an extra example where it helps.

### Rules and the `=` operator

A rule names a pattern:

```
rulename = definition
```

Read `=` as "is defined as." From ATML:

```
atml-unit = 1*ALPHA
```

"An `atml-unit` is one or more `ALPHA` characters." (`ALPHA` and the `1*` are
explained just below.) The name on the left can then be used inside other
rules, exactly as if you had pasted its definition in place.

### Terminals 1: literal characters via `%x`

At the bottom, every rule resolves to actual characters. ABNF's most precise
way to write a character is by its numeric code point in hexadecimal, using
`%x`:

```
atml-zero = %x30          ; bare "0"
```

`%x30` is the character whose hex code is 0x30 — the digit `0`. Writing it this
way is unambiguous about *exactly* which character is meant. TOML's own rules do
the same:

```
minus      = %x2D         ; -
underscore = %x5F         ; _
array-open = %x5B         ; [
```

A **range** uses a hyphen. `%x41-5A` means "any character from 0x41 to 0x5A" —
that is, `A` through `Z`:

```
atml-alpha-no-x = %x41-5A / %x61-77 / %x79-7A   ; A-Z, a-w, y-z
```

(The `/` is "or"; see below. This particular rule — "any letter except lower-
case `x`" — exists to stop `0x...` from being misread as a hex number. A real
design decision, encoded in three ranges.)

### Terminals 2: sequences of bytes with `.`

A dot inside a `%x` value glues code points together into a fixed string.
`%x3A` is a colon (`:`), so:

```
atml-enum-ref = key %x3A.3A atml-enum-symbol
```

`%x3A.3A` is `:` followed by `:` — the `::` separator of an enum reference.
Likewise `%x5B.5D` is `[` followed by `]` — the literal `[]` marker of an enum
declaration:

```
atml-enum-decl = key %x5B.5D keyval-sep atml-enum-list
```

### Terminals 3: literal strings in quotes

ABNF also allows quoted literals, as in TOML's

```
HEXDIG = DIGIT / "A" / "B" / "C" / "D" / "E" / "F"
```

A subtlety worth knowing: in RFC 5234 a quoted string is **case-insensitive**,
so `"A"` also matches a lowercase `a`. That is exactly why `HEXDIG` accepts both
cases without listing `a`–`f`. When case must be pinned down, `%x` is used
instead — which is why ATML's rules lean on `%x` throughout.

### Concatenation: put things in sequence

Writing elements next to each other means "this, then that, in order." In

```
atml-enum-ref = key %x3A.3A atml-enum-symbol
```

a valid enum reference is a `key`, immediately followed by `::`, immediately
followed by an `atml-enum-symbol` — in that order, with nothing missing.

### Alternatives: `/` means "one of"

A slash offers a choice:

```
atml-enum-choice = atml-enum-symbol / val
```

"A choice inside an enum is either a bare symbol *or* an ordinary TOML value."
Alternatives can be chained: `A / B / C` matches any one of the three. TOML's
`key = simple-key / dotted-key` is the same idea.

### Incremental alternatives: `=/` — the heart of ATML

This one operator is why ATML can extend TOML cleanly. `=/` **adds** an
alternative to a rule that already exists, without rewriting it. TOML defines:

```
val = string / boolean / array / inline-table / date-time / float / integer
```

ATML never touches that line. Instead it writes, in its own file:

```
val =/ atml-quantity
val =/ atml-path-ref
val =/ atml-enum-ref
val =/ atml-enum-list
```

Each `=/` line means "a `val` may *also* be this." The original seven TOML value
kinds remain untouched; ATML only appends new possibilities. Because every
extension is an *addition* to an existing rule (or a brand-new `atml-*` rule
that TOML never referenced), it is impossible for the extension to *remove* or
*forbid* anything TOML allowed. That is the mechanical guarantee behind "every
valid TOML document is a valid ATML document": it holds *by construction*, not
by testing alone. TOML's own grammar uses `=/` internally too, e.g.

```
wschar =  %x20    ; Space
wschar =/ %x09    ; Horizontal tab
```

### Repetition: `*`

A star means "repeat." Its general form is `<min>*<max>element`:

- `*element` — zero or more (no lower or upper bound).
- `1*element` — one or more. As in `atml-unit = 1*ALPHA`: a unit is at least one
  letter.
- `2*4element` — between two and four (TOML uses counts like this for the
  digits of a date).

A star can repeat a whole group (see grouping next):

```
atml-enum-symbol = ( ALPHA / underscore ) *( ALPHA / DIGIT / underscore )
```

Read as: one letter-or-underscore, then **zero or more** of
letter-or-digit-or-underscore. This is the classic "identifier" shape — it must
*start* with a letter or `_`, but may *continue* with digits. It is why `mode2`
is a valid symbol but `2mode` is not.

### Grouping: `( ... )`

Parentheses bundle elements so an operator applies to the whole bundle. In the
rule just above, `( ALPHA / underscore )` groups the two alternatives so the
first character is exactly one of them; the separate `*( ... )` group then
governs everything after. Without the parentheses the `*` and `/` would bind
differently and the rule would mean something else entirely. Grouping is how you
control precedence, just like parentheses in arithmetic.

### Optional: `[ ... ]`

Square brackets mean "zero or one of this" — optional. From ATML's number rule:

```
atml-nonzero-dec = [ minus / plus ] digit1-9 *( DIGIT / underscore DIGIT )
```

The `[ minus / plus ]` is an **optional sign**: a leading `+` or `-` may be
present or absent. What follows is a digit `1`–`9`, then any run of further
digits (with optional underscores between them).

> **A trap for TOML users:** in ABNF, `[ ]` means *optional*. This is unrelated
> to TOML's `[ ]`, which builds tables and arrays. Same brackets, different
> world. When you read a grammar, `[ x ]` = "x is optional"; when you read a
> TOML document, `[ x ]` = "a table named x."

### Comments: `;`

In an ABNF file, everything from a semicolon to the end of the line is a comment
for the human reader:

```
atml-zero = %x30          ; bare "0"
digit1-9  = %x31-39       ; 1-9
```

Note that ABNF comments start with `;`, whereas TOML comments start with `#`.
The grammar file and the documents it describes use *different* comment
characters — a small thing that surprises newcomers.

### Core rules you get for free

RFC 5234 predefines a handful of common building blocks so every grammar need
not reinvent them. The ones ATML relies on are:

```
ALPHA  = %x41-5A / %x61-7A     ; A-Z / a-z
DIGIT  = %x30-39               ; 0-9
HEXDIG = DIGIT / "A" / ... / "F"
```

(TOML actually restates these three in its own file rather than importing them;
that is harmless — they are identical — and is the reason a strict ABNF loader
emits a mild "redefinition" warning when it reads the combined grammar.)

---

## Part 3 — Reading real ATML rules end to end

Now the pieces combine. Here are three rules from `atml.abnf`, read fully.

### A symbol (identifier)

```
atml-enum-symbol = ( ALPHA / underscore ) *( ALPHA / DIGIT / underscore )
```

Left to right: a group of *one* letter or underscore, followed by a group
repeated *zero or more* times, each being a letter, digit, or underscore. So
`Active`, `_v`, and `mode2` match; `2mode` and `fast-lane` do not (a digit may
not lead, and `-` is not in the set).

### An enum reference

```
atml-enum-ref = key %x3A.3A atml-enum-symbol
```

A concatenation of three things in order: a TOML `key`, then the literal `::`
(two colons, `%x3A.3A`), then one symbol. This is why `Strategy::Active` is
valid and `Strategy::` (missing symbol) or `a::b::c` (only one `::` allowed) are
not — the rule has room for exactly one `::` and exactly one symbol.

### The quantity rule that avoids a hex clash

This is a rule where ABNF captures a genuinely subtle design decision. A bare
`0` followed by a unit must never be mistaken for a hex number like `0xFF`. The
unit after a lone `0` is therefore defined as:

```
atml-zero-unit  = atml-alpha-no-x *ALPHA
atml-zero-unit =/ %x78 *atml-hex-alpha atml-nonhex-alpha *ALPHA

atml-alpha-no-x   = %x41-5A / %x61-77 / %x79-7A   ; A-Z, a-w, y-z
atml-hex-alpha    = %x41-46 / %x61-66             ; A-F, a-f
atml-nonhex-alpha = %x47-5A / %x67-7A             ; G-Z, g-z
```

Reading the two alternatives:

1. The unit starts with any letter *except* lowercase `x` (`atml-alpha-no-x`),
   then anything. So `0ms`, `0bar`, `0GiB` are fine — they cannot collide with
   `0x...`.
2. *Or* the unit starts with `x` (`%x78`), is followed by some run of hex
   letters, and then **must** contain at least one non-hex letter
   (`atml-nonhex-alpha`, i.e. `g`–`z`/`G`–`Z`) before the rest. That guarantees
   the whole thing cannot be a valid hex literal: `0xenon` passes (the `n` is a
   non-hex letter), while `0xfade` is left to be a hex integer.

You do not need to memorize this rule. The point is that a precise,
human-meaningful constraint — "a quantity must never masquerade as a hex
number" — is expressed entirely in ABNF's small vocabulary of ranges,
alternatives, and repetition, with no prose required.

### Multiple parents in table inheritance

```
atml-inherit-table = std-table-open key ws %x3A ws key
                     *( ws %x2C ws key ) std-table-close
```

Piece by piece: an opening `[` (`std-table-open`), the child `key`, optional
whitespace (`ws`), a colon (`%x3A`), whitespace, the first parent `key`, then
*zero or more* of "(whitespace) comma (whitespace) another parent key"
(`*( ws %x2C ws key )`), and finally the closing `]`. That single repetition
group is what allows `[child : a, b, c]` with any number of parents, while still
requiring at least one.

---

## Part 4 — What ABNF cannot do (and why that is fine)

ABNF describes a **context-free** grammar: each rule matches a local shape,
independent of what appears elsewhere in the document. That power has a precise
edge. ABNF cannot express constraints that depend on *relationships across
distance*, such as:

- "an enum must be declared before it is used,"
- "a referenced enum symbol must actually exist in that enum's definition,"
- "a parent table named here must exist somewhere,"
- "a key must not be defined twice."

None of these is a shape; each is a relationship between two places in the file.
TOML has exactly the same limit — "no key may be defined twice" is *not* in
`toml.abnf`; it is stated in TOML's prose specification. ATML follows the same
division of labour: the grammar in `atml.abnf` fixes the shapes, and the
normative prose in `SPEC.md` supplies the relationships (it calls these
"binding across distance" rules). This is not a weakness of ABNF; it is the
nature of the tool. A grammar is a floor plan, not a building inspector.

---

## Part 5 — Where to look next

- **`grammar/toml-1.1.0.abnf`** — TOML's official grammar, vendored unchanged.
  Reading it is the best next step: every ATML rule builds on names defined
  here (`key`, `val`, `ws`, `std-table-open`, and so on).
- **`grammar/atml-ext.abnf`** — ATML's additions only. Small, and every rule is
  either a `=/` extension or a new `atml-*` rule.
- **`grammar/atml.abnf`** — the two files above, concatenated into the complete
  grammar that tools and tests consume.
- **RFC 5234** — the authority for the notation itself, if you want the formal
  source.

A good habit when reading any rule: start at its name, follow each reference it
mentions down into other rules, and stop when you reach `%x` terminals. At that
point you are looking at actual characters, and the shape the rule describes
becomes concrete.
