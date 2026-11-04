import javax.net.ssl.HostnameVerifier;
import javax.net.ssl.SSLSession;
/** Preserve the logical TLS target when the transport endpoint is an SSH forward. */
public final class TargetHostnameVerifier implements HostnameVerifier {
  static final ThreadLocal<String> target=new ThreadLocal<>();
  private final String hostname;
  public TargetHostnameVerifier(){hostname=target.get();}
  @Override public boolean verify(String ignored,SSLSession session){return hostname!=null&&org.postgresql.ssl.PGjdbcHostnameVerifier.INSTANCE.verify(hostname,session);}
}
