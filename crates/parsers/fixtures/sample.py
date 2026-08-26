"""Fixture for the Python arm of the parser exemplar test.

Every construct here is load-bearing: a class with methods, a class nested in a
class, a function nested in a function, a decorated definition, and two bindings
that must NOT become elements.
"""

MAX_SIDES = 4


def trace(fn):
    return fn


class Rect:
    """A rectangle."""

    SIDES = 4

    def __init__(self, width, height):
        self.width = width
        self.height = height

    def area(self):
        def scale(value):
            return value * 1.0

        return scale(self.width) * self.height

    class Kind:
        SQUARE = "square"


@trace
def main_entry():
    return Rect(1.0, 2.0).area()
