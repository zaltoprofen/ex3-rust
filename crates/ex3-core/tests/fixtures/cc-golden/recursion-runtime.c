unsigned int unsigned_math(unsigned int value)
{
    return (value * 3u) / 2u % 17u;
}

int factorial(int value)
{
    if (value <= 1) {
        return 1;
    }
    return value * factorial(value - 1);
}

int main(void)
{
    return factorial(5) + unsigned_math(19u);
}
