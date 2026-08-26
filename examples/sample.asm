/ Increment COUNT until it reaches zero.
ORG 010

START,  LDA COUNT
        ADD 1
        STA COUNT
        JZA ZERO
        BUN START

ZERO,   HLT
COUNT,  DEC -1
END
