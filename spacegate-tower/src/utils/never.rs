/// Map any type to any type, but this function should be never called.
///
/// this can be a shortcut for `unreachable!()`
pub fn never<A, B>(_: A) -> B {
    unreachable!("never function called")
}
