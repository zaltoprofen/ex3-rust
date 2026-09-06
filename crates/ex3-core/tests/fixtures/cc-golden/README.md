# C compiler v0.1 golden fixtures

The `.asm` files in this directory were generated with the compiler from
commit `aa110ed` (`main` before the Playground v0.2 debug-metadata work).
Tests compare the current compiler output byte-for-byte with these files and
also compare the assembled memory image and symbol table.

Do not update a golden file merely to make a test pass. Update it only for an
intentional code-generation change, and review the Assembly and machine-code
diffs as part of that change.
