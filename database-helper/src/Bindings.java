import com.google.gson.*;
import java.sql.*;
import java.math.BigDecimal;
import java.util.*;
/** Named value parameters only; never substitutes SQL identifiers. */
public final class Bindings {
  record Bound(String sql,List<String> names,String source) {}
  static Bound parse(String sql){
    return parse(sql,"postgresql");
  }
  static Bound parse(String sql,String engine){
    StringBuilder out=new StringBuilder();List<String> names=new ArrayList<>();char quote=0;int comment=0;boolean line=false;String dollar=null;
    for(int i=0;i<sql.length();i++){
      char c=sql.charAt(i),n=i+1<sql.length()?sql.charAt(i+1):0;
      if(line){out.append(c);if(c=='\n')line=false;continue;}
      if(comment>0){out.append(c);if(c=='/'&&n=='*'){comment++;out.append(n);i++;}else if(c=='*'&&n=='/'){comment--;out.append(n);i++;}continue;}
      if(dollar!=null){if(sql.startsWith(dollar,i)){out.append(dollar);i+=dollar.length()-1;dollar=null;}else out.append(c);continue;}
      if(quote!=0){out.append(c);if(c==quote){if(n==quote){out.append(n);i++;}else quote=0;}continue;}
      if(c=='-'&&n=='-'){line=true;out.append(c).append(n);i++;continue;}
      if(c=='/'&&n=='*'){comment=1;out.append(c).append(n);i++;continue;}
      int endQuote=SqlLex.specialQuoteEnd(sql,i,engine);if(endQuote>=0){out.append(sql,i,endQuote);i=endQuote-1;continue;}
      if(c=='\''||c=='"'||(c=='['&&engine.equals("mssql"))){quote=c=='['?']':c;out.append(c);continue;}
      if(c=='$'){var matcher=java.util.regex.Pattern.compile("\\$(?:[A-Za-z_][A-Za-z_0-9]*)?\\$").matcher(sql.substring(i));if(matcher.lookingAt()){dollar=matcher.group();out.append(dollar);i+=dollar.length()-1;continue;}}
      if(c==':'&&n==':'){out.append("::");i++;continue;}
      if(c=='?'&&engine.equals("postgresql")){out.append("??");continue;}
      if(c==':'&&(Character.isLetter(n)||n=='_')){int end=i+2;while(end<sql.length()&&(Character.isLetterOrDigit(sql.charAt(end))||sql.charAt(end)=='_'))end++;names.add(sql.substring(i+1,end));out.append('?');i=end-1;continue;}
      out.append(c);
    }
    return new Bound(out.toString(),names,sql);
  }
  static void bind(PreparedStatement st,Bound bound,JsonObject parameters,Connection connection)throws SQLException{
    for(int i=0;i<bound.names.size();i++){
      JsonElement value=parameters.get(bound.names.get(i));if(value==null)throw new IllegalArgumentException("Eksik parametre: "+bound.names.get(i));
      if(value.isJsonNull())st.setNull(i+1,Types.NULL);
      else if(value.isJsonArray()){Object[] items=new Object[value.getAsJsonArray().size()];for(int j=0;j<items.length;j++)items[j]=value.getAsJsonArray().get(j).isJsonNull()?null:value.getAsJsonArray().get(j).getAsString();st.setArray(i+1,connection.createArrayOf("text",items));}
      else if(value.isJsonPrimitive()){
        JsonPrimitive primitive=value.getAsJsonPrimitive();if(primitive.isBoolean())st.setBoolean(i+1,primitive.getAsBoolean());else if(primitive.isNumber()){BigDecimal n=new BigDecimal(primitive.getAsString());try{st.setLong(i+1,n.longValueExact());}catch(ArithmeticException e){st.setBigDecimal(i+1,n);}}else st.setString(i+1,primitive.getAsString());
      }else throw new IllegalArgumentException("Parametre scalar, null veya text[] olmalı");
    }
  }
}
