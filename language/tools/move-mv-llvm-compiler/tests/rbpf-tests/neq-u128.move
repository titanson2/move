//
module 0x101::Test1 {
  public fun test_neq_u128(a: u128, b: u128): bool {
    a != b
  }
}

script {
  fun main() {
    assert!(0x101::Test1::test_neq_u128(21267647932558653966460912964485513214u128, 21267647932558653966460912964485513215u128), 10);
  }
}
