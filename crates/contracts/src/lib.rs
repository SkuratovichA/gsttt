//! Contract formats for gsttt: the files that define the seams between services rather than implement them.
//!
//! Six connectors, each opening one project at the repository root when the file list holds a file it claims and
//! none otherwise. Their sites are mostly declarations, which is what lets a change to a schema reach the code that
//! serves and calls it.
//!
//! - [`openapi`]: OpenAPI 3 and Swagger 2 documents, and the fragments their `$ref`s reach.
//! - [`graphql`]: schemas, operations and the documents other connectors find embedded in code.
//! - [`proto`]: protobuf services and their `google.api.http` bindings.
//! - [`prisma`]: Prisma models and the environment variables of the datasource.
//! - [`sql`]: migrations and queries, and the SQL other connectors find embedded in code.
//! - [`env`]: `.env` files.

mod common;
mod tree;

pub mod env;
pub mod graphql;
pub mod openapi;
pub mod prisma;
pub mod proto;
pub mod sql;

pub use env::ENV;
pub use graphql::GRAPHQL;
pub use openapi::OPENAPI;
pub use prisma::PRISMA;
pub use proto::PROTO;
pub use sql::SQL;
