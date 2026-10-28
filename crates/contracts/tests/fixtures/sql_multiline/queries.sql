SELECT b.id
  FROM bookings b
  JOIN booking_items i ON i.booking_id = b.id
 WHERE b.id = %s;

SELECT 'it''s' AS note
  FROM carts
 WHERE owner = %(owner)s;
