; Increment COUNT until it reaches zero.
ORG 0x0010

START,  LDA COUNT
        ADD 1
        STA COUNT
        BEQ ZERO
        JMP START

ZERO,   HLT
COUNT,  DEC -1
END
