int global_value = 3;

int identity(int value)
{
    return value;
}

int combine(int first, int second, int third)
{
    int result;

    result = first + second + third;
    if (result > 10) {
        result = -result;
    } else {
        result = result + global_value;
    }
    while (result < 0) {
        result = result + 1;
    }
    return !result;
}

int main(void)
{
    return combine(identity(1), 2, 3);
}
