import sqlite3

# A stand-in for a language file: the host reads the line below as an embedded document.
conn = sqlite3.connect("app.db")

def load():
embed sql SELECT id FROM bookings
