import javax.net.ssl.*;
import java.security.*;
import java.security.cert.*;
import java.nio.file.*;
import java.io.*;
/** A per-connection CA store; never changes JVM-global trust and never accepts all certificates. */
public final class PemTrust implements X509TrustManager {
  private final X509TrustManager delegate;
  private final String hostname;
  public PemTrust(String path)throws Exception{
    String expected="";
    if(path.startsWith("{")){var args=com.google.gson.JsonParser.parseString(path).getAsJsonObject();path=args.get("path").getAsString();expected=args.get("host").getAsString();}
    hostname=expected;
    byte[] bytes=Files.readAllBytes(Path.of(path));if(bytes.length>1_000_000)throw new IOException("CA dosyası çok büyük");
    KeyStore store=KeyStore.getInstance(KeyStore.getDefaultType());store.load(null,null);int i=0;
    for(var cert:CertificateFactory.getInstance("X.509").generateCertificates(new ByteArrayInputStream(bytes)))store.setCertificateEntry("ca"+(i++),cert);
    if(i==0)throw new CertificateException("CA sertifikası yok");
    TrustManagerFactory factory=TrustManagerFactory.getInstance(TrustManagerFactory.getDefaultAlgorithm());factory.init(store);
    delegate=(X509TrustManager)factory.getTrustManagers()[0];
  }
  public void checkClientTrusted(X509Certificate[] c,String a)throws CertificateException{delegate.checkClientTrusted(c,a);}
  public void checkServerTrusted(X509Certificate[] c,String a)throws CertificateException{delegate.checkServerTrusted(c,a);if(!hostname.isEmpty())verifyName(c[0],hostname);}
  static void verifyName(X509Certificate cert,String host)throws CertificateException{
    String h=host.toLowerCase(java.util.Locale.ROOT);boolean ip=h.matches("[0-9.]+")||h.contains(":");var sans=cert.getSubjectAlternativeNames();boolean dns=false;
    if(sans!=null)for(var san:sans){int type=(Integer)san.get(0);if(type==2)dns=true;if((ip&&type==7)||(!ip&&type==2)){String name=san.get(1).toString().toLowerCase(java.util.Locale.ROOT);if(h.equals(name)||(!ip&&h.indexOf('.')>0&&name.startsWith("*.")&&name.substring(1).equals(h.substring(h.indexOf('.')))))return;}}
    if(!ip&&!dns)try{for(var rdn:new javax.naming.ldap.LdapName(cert.getSubjectX500Principal().getName()).getRdns())if(rdn.getType().equalsIgnoreCase("CN")&&h.equals(rdn.getValue().toString().toLowerCase(java.util.Locale.ROOT)))return;}catch(javax.naming.InvalidNameException ignored){}
    throw new CertificateException("TLS sertifikası hedef sunucu adıyla eşleşmiyor");
  }
  public X509Certificate[] getAcceptedIssuers(){return delegate.getAcceptedIssuers();}
  static SSLContext context(String path)throws Exception{SSLContext c=SSLContext.getInstance("TLS");c.init(null,new TrustManager[]{new PemTrust(path)},new SecureRandom());return c;}
}
