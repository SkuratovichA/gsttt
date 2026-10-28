from sqlalchemy import text as t


def find(connection, id):
    return connection.execute(t("SELECT id FROM bookings WHERE id = :id"), {"id": id})
