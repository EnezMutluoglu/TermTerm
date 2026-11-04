import { describe, expect, it } from "vitest";
import { documentData } from "../src/database/sqlDocuments";
describe("SQL documents", () => {
  it("stores ordered instructions but never execution parameters or results", () => {
    const steps = [
      {
        title: "  Oturumlar ",
        description: "Gözden geçir",
        sql: "SELECT :name",
        parameters: { name: "secret" },
      },
      { title: "İkinci", description: "", sql: "SELECT 2" },
    ];
    const data = documentData(
      "databaseRunbook",
      "  Kontrol ",
      "Türkçe",
      "db",
      "folder",
      steps,
    );
    expect(data).toEqual({
      label: "Kontrol",
      description: "Türkçe",
      connectionId: "db",
      groupId: "folder",
      steps: [
        {
          title: "Oturumlar",
          description: "Gözden geçir",
          sql: "SELECT :name",
        },
        { title: "İkinci", description: "", sql: "SELECT 2" },
      ],
    });
    expect(JSON.stringify(data)).not.toContain("secret");
  });
  it("keeps saved SQL intact and rejects empty documents", () => {
    const step = {
      title: "One",
      description: "",
      sql: "SELECT N'İstanbul';\nGO\nSELECT 2",
    };
    expect(
      documentData("savedQuery", "Test", "", "db", "", [step]),
    ).toHaveProperty("sql", step.sql);
    expect(() =>
      documentData("savedQuery", "", "", "db", "", [step]),
    ).toThrow();
    expect(() =>
      documentData("databaseRunbook", "Test", "", "db", "", []),
    ).toThrow();
    expect(() =>
      documentData(
        "databaseRunbook",
        "Test",
        "",
        "db",
        "",
        Array(51).fill(step),
      ),
    ).toThrow();
  });
});
