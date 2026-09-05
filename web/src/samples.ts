export interface SampleProgram {
  id: string;
  name: string;
  source: string;
}

export const SAMPLE_PROGRAMS: SampleProgram[] = [
  {
    id: "factorial",
    name: "Recursive factorial",
    source: `int fact(int n) {
    if (n <= 1) return 1;
    return n * fact(n - 1);
}

int main(void) {
    return fact(5);
}
`,
  },
  {
    id: "serial",
    name: "Serial output",
    source: `void putchar(int c);

int main(void) {
    putchar(72);
    putchar(105);
    putchar(10);
    return 0;
}
`,
  },
  {
    id: "loop",
    name: "Loop and breakpoint",
    source: `int main(void) {
    int total = 0;
    int i = 1;
    while (i <= 10) {
        total = total + i;
        i = i + 1;
    }
    return total;
}
`,
  },
];
