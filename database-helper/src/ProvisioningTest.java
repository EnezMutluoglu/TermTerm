import com.google.gson.JsonObject;
import java.util.*;
public final class ProvisioningTest {
 public static void main(String[] args){
  var passwords=new HashSet<String>();for(int i=0;i<10000;i++){String p=Provisioning.password();if(p.length()!=21||!p.matches(".*[A-Z].*")||!p.matches(".*[a-z].*")||!p.matches(".*[0-9].*")||!p.matches(".*[!#%+_=\\-].*")||!passwords.add(p))throw new AssertionError("Password invariant");}
  var p=new JsonObject();p.addProperty("name","example_api_dev");var in=Provisioning.input(p);
  for(String e:List.of("postgresql","mssql","oracle")){String sql=Provisioning.steps(e,in).stream().map(Provisioning.Step::sql).reduce("",(a,b)->a+"\n"+b);if(!sql.contains("<OWNER_PASSWORD>")||!sql.contains("<APP_PASSWORD>")||sql.contains("azure_pg_admin")||sql.contains("rds_superuser"))throw new AssertionError("Template secret/escalation");}
  for(String bad:List.of("a;drop","a\"","pg_admin","ora_test","x".repeat(49),"a-b","")){p.addProperty("name",bad);try{Provisioning.input(p);throw new AssertionError("Name accepted");}catch(IllegalArgumentException expected){}}
  if(!ChangePlans.expired(5,5+ChangePlans.TTL)||ChangePlans.expired(5,4+ChangePlans.TTL))throw new AssertionError("TTL");
  System.out.println("ProvisioningTest: password/identifier/template/TTL invariants passed (10,000 passwords; no secret output)");
 }
}
