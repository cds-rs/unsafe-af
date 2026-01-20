#set page(margin: 1.5cm)
#set text(font: "New Computer Modern", size: 11pt)
#set heading(numbering: "1.")

= Undefined Behavior as a Domain Restriction

== TL;DR

*Undefined behavior is not "wrong output."* \
It means the program has stepped _outside the compiler's contract_. \
Unreachable UB is allowed; reachable UB voids all guarantees.

This document tries to capture the Rektoff Cohort 3, office hours conversation around Rust's undefined behavior. 

== Model

Let:

- $P$ := the set of all *syntactically valid* Rust programs
- $S subset P$ := programs for which *no possible execution* can trigger UB
- $U := P without S$
- $C$ := the set of machine-code executables

Model the compiler as a *partial function*:

$"compile": S -> C$

It is defined only on $S$. Programs in $U$ lie outside the compiler’s domain of definition.

#quote(block: true)[
  _The compiler maps valid Rust programs to well-defined executables._
]

== Reachability Is the Boundary

Membership in $S$ is a property of *executions* (e), not syntax.

Formally:

$ p in S quad "if" quad forall e in op("Exec")(p), e "does not trigger UB" $

Unreachable undefined behavior is permitted; reachable undefined behavior is not.

== Why This Matters

=== Unreachable UB: allowed (in $S$)

```rust
if false {
    unsafe {
        *(0 as *mut u8) = 1;
    }
}
```

- Syntactically valid (in $P$)
- The branch is provably unreachable
- No execution can trigger UB
- Therefore the program is in $S$, and compile is well-defined

=== Reachable UB: forbidden (in $U$)

```rust

if cond { // get rekt!
    unsafe {
        *(0 as *mut u8) = 1;
    }
}
```

- There exists an execution where `cond == true`
- UB is reachable
- Therefore the program is in $U$
- The compiler's semantic guarantees no longer apply

This holds even if cond is “always false in practice”.

== Diagram (Mental Model)

#align(center)[
  #block(stroke: 1pt, inset: 0pt)[
    #block(inset: 6pt, width: 100%, fill: luma(240))[
      #align(center)[*P* — all syntactically valid Rust programs]
    ]
    #grid(
      columns: (1fr, 1fr),
      block(
        fill: rgb(220, 245, 220),
        stroke: (right: 1pt),
        inset: 12pt,
        height: 6em,
        width: 100%,
      )[
        #align(center)[
          *S* \
          #text(size: 9pt)[no execution triggers UB] \
          #v(0.5em)
          $"compile": S -> C$ #text(fill: green)[✓]
        ]
      ],
      block(
        fill: rgb(255, 230, 230),
        inset: 12pt,
        height: 6em,
        width: 100%,
      )[
        #align(center)[
          *P \\ S* \
          #text(size: 9pt)[reachable UB exists] \
          #v(0.5em)
          #text(fill: red)[undefined ✗]
        ]
      ],
    )
  ]
  #v(0.3em)
  #text(size: 18pt)[↓] #h(1em) #text(size: 9pt)[(only from S)]
  #v(0.3em)
  #block(stroke: 1pt, inset: 10pt, fill: rgb(230, 240, 255))[
    *C* — well-defined executables
  ]
]

Only programs inside $S$ are valid inputs to the compiler-as-a-function.

== Consequences

$"UB" != "incorrect output"$

$"UB" = "execution outside the compiler's contract"$

If a program admits an execution that triggers UB:
- The program lies outside the compiler's domain of definition
- The compiler always assumes its input satisfied Rust's rules
- No semantic guarantees apply to the resulting behavior

== Contrast: Rust vs C/C++

Key difference: where the boundary is enforced

*Rust*
- UB is defined in terms of possible executions
- Unreachable UB is explicitly allowed
- Safety rules are designed to preserve soundness under aggressive optimization (the whole pipeline of AST -> MIR -> ... LLVM) operates on trusted inputs. GIGO caveats.

*C / C++*
- UB is often tied more loosely to syntax and local reasoning
- "This code never runs" arguments are more fragile. (Oh yea? prove it!)
- Compilers may generate UB in ways that surprise even experienced developers

Put differently:

#quote(block: true)[
Rust draws a hard semantic boundary around what may execute. 
C and C++ often leave that boundary implicit, and leaky.
]

== Conclusion

#quote(block: true)[
A Rust program is valid only if every possible execution stays within the compiler’s assumptions.
]

== The obvious lingering question:
- How do you prove a rust program has no UB?
