from sqlalchemy import text


def named(connection, query):
    connection.execute(text(query))
    connection.execute(text("SELECT id FROM bookings"))
