def run(cursor, self):
    cursor.execute("SELECT id FROM one")
    self.cursor.execute("SELECT id FROM two")
    self.db.cursor.execute("SELECT id FROM three")
    self.app.db.cursor.execute("SELECT id FROM four")
