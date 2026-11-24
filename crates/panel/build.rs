// `sqlx::migrate!` embeds the migration files at compile time; without this, adding a new
// file to `migrations/` would not trigger a rebuild and the new migration would be skipped.
fn main() {
    println!("cargo:rerun-if-changed=migrations");
}
