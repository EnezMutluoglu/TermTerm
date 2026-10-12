// Synthetic UI events only; real Tauri/TLS/DB flows have a separate native suite.
import React from "react";
import { createRoot } from "react-dom/client";
import "../../src/style.css";
const w = window as any;
w.isTauri = true;
w.calls = [];
const content = (kind: string, parentId: string | null, name: string) => ({
  kind,
  parentId,
  name,
  address: kind === "host" ? "127.0.0.1" : null,
  protocol: kind === "host" ? "ssh" : null,
  port: kind === "host" ? 22 : null,
  usernamePolicy: kind === "host" ? "personal" : null,
  username: null,
  deleted: false,
});
const rows = [
  {
    id: "folder",
    revision: 1,
    content: content("folder", null, "Üretim"),
    permissions: ["read", "edit"],
  },
  {
    id: "host",
    revision: 1,
    content: content("host", "folder", "Laboratuvar"),
    permissions: ["read", "edit"],
  },
];
w.__TAURI_INTERNALS__ = {
  metadata: {
    currentWindow: { label: "main" },
    currentWebview: { label: "main" },
  },
  invoke: async (command: string, { action: a, ...args }: any = {}) => {
    w.calls.push({ command, kind: a?.kind, action: a });
    if (command === "plugin:event|listen") return 1;
    if (command === "plugin:event|unlisten") return null;
    if (command === "team_migration_preview") {
      w.migrationArguments={path:args.path,passwordProvided:!!args.password,mappings:args.mappings,expectedSourceSha256:args.expectedSourceSha256,expectedTargetFingerprint:args.expectedTargetFingerprint};
      if(w.migrationFailure)throw Error("Wrong password or damaged encrypted data");
      if(w.migrationStale && args.mappings)throw Error("Hedef üyeler/yetkiler değişti; arşivi yeniden önizleyin");
      if(w.migrationHold)await new Promise<void>(r=>{w.releaseMigration=r;});
      return {sourceTeamName:"Eski İstanbul",sourceSha256:"abc123",capturedAt:"2026-10-08T00:00:00Z",issueRecords:1,
        vaults:[{sourceId:"old-vault",name:"Eski kasa",recordCount:2}],
        members:[{sourceId:"old-owner",username:"leader",role:"owner",active:true,targetAccountId:args.mappings?.[0]?.targetAccountId??null,mappingIssues:args.mappings?.length?[]:["Hedef hesabı açıkça seçin"]}],
        targetAccounts:[{accountId:"new-owner",username:"leader",displayName:"Yeni lider",role:"owner",status:"active"}],
        targetFingerprint:"target123",accountsReady:!!args.mappings?.length,unresolvedAccounts:args.mappings?.length?0:1,
        rules:[{kind:"acl",sourceAccountId:"old-owner",permission:"export",effect:"deny"}],
        records:[{sourceId:"old-folder",kind:"group",name:"Üretim",deleted:false,issues:[],unsupportedFields:[]},
        {sourceId:"old-host",kind:"host",name:"Eski SSH",deleted:false,issues:["Ek alanlar için açık dönüşüm kararı gerekli"],unsupportedFields:["proxy"]}],
        limitations:["Önizleme; hedefe henüz yazılmaz ve eski giriş kapatılmaz"]};
    }
    if (command === "team_v2_compatibility") {
      if(w.holdPolicyTeam===args.profile.teamId) return new Promise((resolve,reject)=>{w.resolvePolicy=resolve;w.rejectPolicy=reject;});
      if(w.policyNetworkFailure) throw Error("network_unavailable");
      if(w.requiredPolicy?.required) w.calls.push({command:"native-policy-logout"});
      return w.requiredPolicy ?? {required:false};
    }
    if (command === "team_v2_saved_login") return w.savedLogin??null;
    if (command === "team_v2_forget_login") { w.savedLogin=null; return null; }
    if (command === "team_v2_login_remembered") {
      if(w.rememberFailure) throw Error("authentication_denied");
      return {activationRequired:false};
    }
    if (command === "update_info") return {version:"0.4.1-dev.5",channel:"development",enabled:false,repository:"",packageSupported:true};
    if (command === "team_v2_logout") return null;
    if (command === "agent_identities") {
      if(w.agentFailure)throw Error("SSH signing agent is unavailable");
      return [{agentKey:"SHA256:"+"A".repeat(43),label:"Disposable agent key"},{agentKey:"CERT-SHA256:"+"B".repeat(43),label:"Disposable agent key",certificate:true}];
    }
    if (command === "app_info") return { home: w.sftpFixture ? "C:/test" : "" };
    if (command === "file_list") return [];
    if (command === "team_v2_login") return { activationRequired: false };
    if(command === "team_v2_submit_drafts") {
      if(w.deliveryFailure)throw Error("candidate_delivery_expired");
      return {delivered:1,remaining:0,awaitingDecision:1};
    }
    if (a?.kind === w.hold) {
      await new Promise<void>((resolve) => {
        w.release = resolve;
      });
    }
    switch (a?.kind) {
      case "clientPolicy":return w.policy??{revision:0,minimumClientVersion:"0.4.1-dev.5",selectedVersion:null,versions:["0.4.1-dev.5","0.4.2"]};
      case "setClientPolicy":
        if(w.policyConflict)throw Error("revision_conflict");
        w.policy={revision:a.expectedRevision+1,selectedVersion:a.version};return w.policy;
      case "sharedRecoveryOpen": return {label:"Shared test key",revision:4,recipients:[{accountId:"leader",displayName:"Lider",connect:true,reveal:true,export:true},{accountId:"operator",displayName:"Operatör",connect:true,reveal:false,export:false}]};
      case "sharedRecoveryFinish":
        if(w.sharedRecoveryFailure&&!a.cancel)throw Error("preview_changed");
        return a.cancel?{cancelled:true}:{revision:5};
      case "rotationOpen": return {operationId:"rotation",status:"running",total:8,remaining:8};
      case "rotationStep":
        if(w.rotationFailure)throw Error("Recovery ciphertext invalid");
        return {operationId:"rotation",status:"running",total:8,remaining:0};
      case "rotationFinish": return {completed:true};
      case "rotationClose": return {closed:true};
      case "conflicts":
        return {conflicts:w.candidateFixture && !w.decisionApplied && !a.after ? [{id:"creation-decision",resourceId:"offline-new",expectedRevision:0,content:content("host","folder","Çevrimdışı yeni makine"),error:null}]:[],nextAfter:null};
      case "conflictCurrent":
        return {id:"offline-new",vaultId:"vault",revision:0,content:null};
      case "decideConflict":
        if(w.decisionFailure)throw Error("revision_conflict");
        if(a.currentRevision!==0 || a.resourceId!=="offline-new")throw Error("Invalid decision scope");
        w.decisionApplied=true;w.decisionChoice=a.replacement?"approve":"reject";
        return {status:"resolved",revision:a.replacement?1:0};
      case "journalStatus":
        return w.journal ?? {pending:0,conflicts:0,quarantined:0,editRemainingMs:0};
      case "syncJournal":
        if(w.failSync)throw Error("network_unavailable");
        w.journal={pending:0,conflicts:0,quarantined:0,editRemainingMs:3600000};
        return {delivered:2,journal:w.journal};
      case "reviewPins":
        return {
          items: w.pinReleased
            ? []
            : [
                {
                  id: "9007199254740996",
                  displayName: "Old member",
                  accountStatus: "disabled",
                  name: "Laboratuvar",
                  resourceId: "host",
                  revision: 1,
                  purpose: "temporary",
                  createdAt: "2026-10-06T10:00:00Z",
                },
              ],
          nextAfter: null,
        };
      case "preparePinRelease":
        return {
          id: "pin-plan",
          pins: 1,
          items: [
            {
              id: "9007199254740996",
              displayName: "Old member",
              resourceId: "host",
              revision: 1,
              purpose: "temporary",
            },
          ],
        };
      case "finishPinRelease":
        if (a.cancel) return { status: "cancelled" };
        w.pinReleased = true;
        return { released: 1, pruned: 1 };
      case "storage":
        return {
          encryptedPayloadBytes: "1048576",
          records: 2,
          versions: 4,
          auditDays: 90,
          lastMaintenance: null,
          nextMaintenance: null,
        };
      case "audit":
        return {
          events: [
            {
              id: "9007199254740995",
              actor: "Leader",
              action: "credential.share",
              outcome: "succeeded",
              serverAt: "2026-10-06T10:00:00Z",
            },
          ],
          nextBefore: null,
        };
      case "maintenance":
        if (w.maintenanceFailure) throw Error("Maintenance unavailable");
        return {
          auditDeleted: 8,
          accessPlansDeleted: 2,
          sharePlansDeleted: 0,
          auditDays: 90,
        };
      case "recoveryPending":
        return [
          {
            id: "recovery-op",
            actor: "Leader",
            createdAt: "2026-10-06T10:00:00Z",
          },
        ];
      case "recoveryVerify":
        return {
          operationId: "recovery-op",
          displayName: "Recipient",
          publicKey: "ab".repeat(32),
          total: 1,
          verification: true,
        };
      case "recoveryStep":
        if (w.recoveryFailure) throw Error("Recovery ciphertext invalid");
        return {
          completed: true,
          processed: 1,
          total: 1,
          verificationRequired: false,
        };
      case "recoveryCancel":
        return { cancelled: true };
      case "proxyStatus": return w.proxyStatus??{credentialId:null,revision:0};
      case "saveProxy":
        if(w.proxyFailure)throw Error("revision_conflict");
        w.proxyStatus=a.secret?{credentialId:"proxy-id",revision:1,username:a.secret.username,endpoint:a.secret.endpoint}:{credentialId:null,revision:0};return {};
      case "resource": return {revision:3,content:content("host",null,"Current"),permissions:["read","edit"]};
      case "history":return {versions:[{revision:2,createdAt:"2026-10-12T00:00:00Z",actorId:"owner",content:content("host",null,"Earlier"),error:null}],pins:[]};
      case "prepareRestore":return {planId:"restore-plan",hops:[{id:"jump",name:"Historical jump"}],recipientCount:2};
      case "profiles":
        return {
          default: w.brokenDefault
            ? {
                hostId: null,
                revision: 2,
                method: "credential",
                credentialId: "private-key",
                username: null,
                unavailable: true,
              }
            : null,
          hosts: [],
        };
      case "saveProfile":
        return { revision: 3 };
      case "profileHistory":
        return [{hostId:null,revision:1,method:"prompt",credentialId:null,username:"onceki_kullanici",unavailable:false},
          {hostId:null,revision:0,method:"prompt",credentialId:null,username:null,unavailable:true}];
      case "restoreProfile":
        if(w.profileConflict)throw Error("revision_conflict");
        return {revision:3};
      case "overview":
        return {
          offline: !!w.offlineFixture,
          teamName: "UI laboratuvarı",
          displayName: "Lider",
          role: "owner",
          accountId: "owner",
        };
      case "delegations":
        return { aclRevision: 1, delegations: [] };
      case "vaults":
        return [{ id: "vault", name: "Ortak kasa" }];
      case "members":
        return [
          {
            accountId: "owner",
            username: "leader",
            displayName: "Lider",
            role: "owner",
            status: "active",
          },
          {
            accountId: "user",
            username: "member",
            displayName: "Üye",
            role: "operator",
            status: "active",
          },
          {
            accountId: "manager",
            username: "manager",
            displayName: "Kapsam yöneticisi",
            role: "manager",
            status: "active",
          },
        ];
      case "chainConfiguration": return {hops:[]};
      case "prepareMove":return {name:"DB İstanbul",requiredKeys:2,affectedAccess:1};
      case "moveImpact":return {items:[{resourceId:"host",accountId:"user",displayName:"Üye",before:["read","connect"],after:[]}],nextAfter:null,chainReferences:1,sharedIdentities:1};
      case "stageMove":return {processed:2,total:2,ready:true};
      case "finishMove":if(w.moveFailure&&!a.cancel)throw Error("preview_changed");return {status:a.cancel?"cancelled":"applied"};
      case "prepareChain": return {planId:"chain-plan",recipientCount:2,hops:a.hops.map((id:string)=>({id,name:"Jump İstanbul",revision:1}))};
      case "finishChain": if(w.chainFailure&&!a.cancel)throw Error("revision_conflict");return {status:a.cancel?"cancelled":"applied",revision:4};
      case "saveResource":
        if(w.documentFailure)throw Error("permission_denied");
        if(a.content.document){const old=rows.findIndex(r=>r.id===a.id);const r={id:a.id,revision:a.expectedRevision+1,content:a.content,permissions:["read","edit"]};if(old>=0)rows[old]=r;else rows.push(r);return {status:"applied"};}
        return {status:"applied"};
      case "resources":
        return { records: w.documentFixture ? [...rows,{id:"allowed",revision:1,content:content("host","folder","İzinli hedef"),permissions:["read","connect"]}] : w.sftpFixture ? [...rows,{id:"allowed",revision:1,content:content("host","folder","İzinli SFTP"),permissions:["read","connect"]}] : w.chainFixture ? [...rows,{id:"jump",revision:1,content:content("host",null,"Jump İstanbul"),permissions:["read","connect"]}] : rows, nextAfter: null };
      case "prepareAccess":
        return { id: "plan", affectedResources: 2, requiredKeys: 3 };
      case "accessImpact":
        return {
          records: [
            {
              id: "host",
              kind: "host",
              before: [],
              after: ["read", "connect"],
            },
          ],
          nextAfter: null,
        };
      case "stageAccess":
        return { staged: 3, nextAfter: null };
      case "finishAccess":
        return { status: "applied" };
      case "createMember":
        return {
          accountId: "created",
          temporaryPassword: "Disposable-Once-12345!",
        };
      case "credentials":
        return [
          {
            id: "private-key",
            label: "Private test key",
            kind: "ssh_key",
            revision: 1,
            visibility: "private",
          },
          ...(w.shared
            ? [
                {
                  id: "shared-key",
                  label: "Shared test key",
                  kind: "ssh_key",
                  revision: 1,
                  visibility: "shared",
                  permissions: ["connect"],
                  canRemove: false,
                  canUpdate: Boolean(w.canUpdate),
                },
              ]
            : []),
        ];
      case "shareCandidates":
        return [{ accountId: "user", displayName: "Member" }];
      case "sharedUpdateOptions":
        return {
          scopeId: "folder",
          revision: 1,
          recipients: [
            {
              accountId: "user",
              displayName: "Member",
              connect: true,
              reveal: false,
              export: false,
            },
          ],
        };
      case "prepareSharedUpdate":
        return {
          id: "update-plan",
          recipients: a.recipients.map((r: any) => ({
            ...r,
            displayName: "Member",
          })),
        };
      case "finishSharedUpdate":
        if (w.updateFailure) throw Error("preview_changed");
        return { credentialId: "shared-key", revision: 2 };
      case "cancelSharedUpdate":
        return { status: "cancelled" };
      case "prepareShare":
        return {
          id: "share-plan",
          recipients: [
            ...a.recipients.map((r: any) => ({ ...r, displayName: "Member" })),
            {
              accountId: "owner",
              displayName: "Owner",
              connect: true,
              reveal: true,
              export: true,
            },
          ],
        };
      case "finishShare":
        w.shared = true;
        return { credentialId: "shared-key", revision: 1 };
      case "cancelShare":
        return { status: "cancelled" };
      case "historySelections":
        return [];
      case "setDelegation":
        return { aclRevision: 2, permissions: a.permissions };
      default:
        throw Error("Unexpected fixture command");
    }
  },
};
const { default: TeamMigration } = await import("../../src/TeamMigration");
const { default: TeamService } = await import("../../src/TeamService");
const {default:TeamProxy}=await import("../../src/TeamProxy");
const {default:TeamHistory}=await import("../../src/TeamHistory");
function Harness() {
  const [closed, setClosed] = React.useState(false);
  if(new URLSearchParams(location.search).has("proxy"))return <TeamProxy root="root" hostId="hop" endpoint={{kind:"http",host:"proxy.invalid",port:8080,authenticate:true}}/>;
  if(new URLSearchParams(location.search).has("history"))return <TeamHistory id="host" name="Host" onClose={()=>setClosed(true)} onChanged={()=>{}}/>;
  if (new URLSearchParams(location.search).has("legacy-preview")) return <TeamMigration/>;
  return closed ? (
    <>Oturum kapandı</>
  ) : (
    <TeamService onBack={() => setClosed(true)} />
  );
}
createRoot(document.getElementById("root")!).render(<Harness />);
