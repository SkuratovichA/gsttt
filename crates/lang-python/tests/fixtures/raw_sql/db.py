import sqlite3


def find(id):
    connection = sqlite3.connect("app.db")
    cursor = connection.cursor()
    cursor.execute("SELECT id FROM bookings WHERE id = %s", (id,))
    return cursor.fetchone()


def find_many(ids):
    cursor = sqlite3.connect("app.db").cursor()
    cursor.execute(
        """
        SELECT id
        FROM bookings
        WHERE id IN %s
        """,
        (ids,),
    )
    return cursor.fetchall()


def find_joined(id):
    cursor = sqlite3.connect("app.db").cursor()
    cursor.execute(
        "SELECT id "
        "FROM bookings "
        "WHERE id = %s",
        (id,),
    )
    return cursor.fetchone()


def find_added(id):
    cursor = sqlite3.connect("app.db").cursor()
    cursor.execute(
        "SELECT id "
        + "FROM bookings "
        + "WHERE id = %s",
        (id,),
    )
    return cursor.fetchone()


def find_escaped(id):
    cursor = sqlite3.connect("app.db").cursor()
    cursor.execute("SELECT id\nFROM bookings\nWHERE id = %s", (id,))
    return cursor.fetchone()
