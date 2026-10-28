from typing import TYPE_CHECKING

if TYPE_CHECKING:
    import a

try:
    import b
except ImportError:
    b = None
