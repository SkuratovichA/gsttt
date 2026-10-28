// A stand-in for a language file: the host reads the line below as an embedded document.
import { request } from "./request";

export const note = "the document is on line 5";
embed graphql query { bookings { id } }
