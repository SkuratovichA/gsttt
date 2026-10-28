def held(cursor, pool, queries, task):
    pool.execute(task)
    pool.execute(queries[0])
    cursor.executemany(queries.insert, [(task,)])
    cursor.execute(compose(task))
    cursor.execute()
