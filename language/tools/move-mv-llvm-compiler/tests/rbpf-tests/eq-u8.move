//
module 0x101::Test1 {
  public fun test_eq_u8(a: u8, b: u8): bool {
    a == b
  }
}

script {
  fun main() {
    assert!(0x101::Test1::test_eq_u8(255u8, 255u8), 10);
  }
}
