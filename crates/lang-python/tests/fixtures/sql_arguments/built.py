def built(cursor, table, task):
    cursor.execute(f"SELECT id FROM {table}")
    cursor.execute("SELECT id FROM " + table)
    cursor.execute("SELECT id FROM bookings WHERE id = %s" % task)
    cursor.execute(("SELECT id FROM " + table + " WHERE id = {}").format(task))
    cursor.execute("SELECT id FROM bookings")
