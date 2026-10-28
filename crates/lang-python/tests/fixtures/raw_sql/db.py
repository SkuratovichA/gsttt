import sqlite3


def find(id):
    connection = sqlite3.connect("app.db")
    cursor = connection.cursor()
    cursor.execute("SELECT id FROM bookings WHERE id = %s", (id,))
    return cursor.fetchone()
