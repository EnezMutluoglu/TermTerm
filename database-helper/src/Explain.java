import com.google.gson.*;
import java.sql.*;
import java.util.*;
public final class Explain {
  static Object run(Connection c,String engine,JsonObject p)throws Exception{
    if(engine.equals("oracle")){
      String id=Main.str(p,"sqlId","");if(!id.matches("[0-9a-z]{13}"))throw new IllegalArgumentException("Geçerli SQL_ID gerekli");int child=Main.integer(p,"childNumber",0);if(child<0)throw new IllegalArgumentException("Child number negatif olamaz");
      try(PreparedStatement st=c.prepareStatement("SELECT plan_table_output FROM TABLE(DBMS_XPLAN.DISPLAY_CURSOR(?,?,'TYPICAL'))")){st.setString(1,id);st.setInt(2,child);st.setQueryTimeout(15);try(ResultSet rs=st.executeQuery()){return Main.rows(rs,1000,new Main.Budget(2_000_000),1_000_000);}}
    }
    String sql=Main.str(p,"sql","").trim();
    // This entrypoint only accepts one SELECT; batch control is never sent under SHOWPLAN.
    var parts=SqlScript.split(sql,"postgresql");if(parts.size()!=1)throw new IllegalArgumentException("Plan için tek SELECT seçin");sql=parts.get(0);
    if(!SqlScript.leadingSql(sql).matches("(?is)^select\\b.*")||sql.matches("(?is).*(\\binto\\b|\\bshowplan\\b|\\bexecute\\b|\\bexec\\b).*"))throw new IllegalArgumentException("Bu sürümde plan yalnız tek SELECT için kullanılabilir");
    if(engine.equals("postgresql")){c.setReadOnly(true);c.setAutoCommit(false);try(Statement st=c.createStatement()){st.setQueryTimeout(15);try(ResultSet rs=st.executeQuery("EXPLAIN (FORMAT JSON, ANALYZE FALSE) "+sql)){return Main.rows(rs,1000,new Main.Budget(2_000_000),1_000_000);}}finally{c.rollback();}}
    try(Statement st=c.createStatement()){st.setQueryTimeout(15);st.execute("SET SHOWPLAN_XML ON");try{try(ResultSet rs=st.executeQuery(sql)){return Main.rows(rs,1000,new Main.Budget(2_000_000),1_000_000);}}finally{st.execute("SET SHOWPLAN_XML OFF");}}
  }
}
