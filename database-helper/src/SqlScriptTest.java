import java.util.*;
public final class SqlScriptTest {
  static void check(boolean value){if(!value)throw new AssertionError();}
  public static void main(String[] args){
    check(SqlScript.split("SELECT ';'; SELECT 2;", "postgresql").size()==2);
    check(SqlScript.split("DO $body$ BEGIN RAISE NOTICE ';'; END $body$; SELECT 1;", "postgresql").size()==2);
    check(SqlScript.split("SELECT 1 /* outer /* inner ; */ still ; */; SELECT 2;", "postgresql").size()==2);
    check(SqlScript.split("DECLARE @a int=1; SELECT @a;\nGO\nSELECT 'GO';", "mssql").size()==2);
    check(SqlScript.split("SELECT 'line\nGO\nline';", "mssql").size()==1);
    check(SqlScript.split("BEGIN NULL; NULL; END;\n/\nSELECT 1 FROM dual;", "oracle").size()==2);
    boolean bad=false;try{SqlScript.split("SELECT 'oops", "postgresql");}catch(IllegalArgumentException e){bad=true;}check(bad);
    var b=Bindings.parse("SELECT :name::text, ':ignored', $$:ignored$$ /* :ignored */ -- :ignored\n, :number");
    check(b.names().equals(List.of("name","number")));check(b.sql().contains("?::text"));
    check(Bindings.parse("BEGIN x := :value; END;").names().equals(List.of("value")));
    check(SqlScript.split("SELECT q'[it's; :value]' FROM dual; SELECT 1 FROM dual;","oracle").size()==2);
    check(Bindings.parse("SELECT q'[it's; :value]', :real FROM dual","oracle").names().equals(List.of("real")));
    check(SqlScript.split("SELECT E'it\\'s; :value'; SELECT 2;","postgresql").size()==2);
    check(Bindings.parse("SELECT E'it\\'s; :value', :real","postgresql").names().equals(List.of("real")));
    check(SqlScript.split("-- comment\n/* nested /* x */ */ BEGIN NULL; NULL; END;\n/\nSELECT 1 FROM dual;","oracle").size()==2);
    check(SqlScript.split("SELECT 1 FROM dual; BEGIN NULL; NULL; END;\n/", "oracle").size()==2);
    check(SqlScript.split("SELECT 1; -- last comment", "postgresql").size()==1);
    check(SqlScript.split("SELECT 1;\nGO -- batch\nSELECT 2;", "mssql").size()==2);
    System.out.println("18 SQL boundary and binding tests passed");
  }
}
