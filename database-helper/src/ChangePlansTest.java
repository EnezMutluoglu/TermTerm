/** Deterministic expiry boundary, no sleeping or production test switch. */
public final class ChangePlansTest {
  public static void main(String[] args) {
    if(ChangePlans.expired(42,42+ChangePlans.TTL-1))throw new AssertionError("early expiry");
    if(!ChangePlans.expired(42,42+ChangePlans.TTL))throw new AssertionError("boundary expiry");
    long wrapped=Long.MAX_VALUE-100;
    if(!ChangePlans.expired(wrapped,wrapped+ChangePlans.TTL))throw new AssertionError("monotonic wrap");
    System.out.println("ChangePlansTest: 3 expiry assertions passed");
  }
}
