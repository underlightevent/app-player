import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { AlertTriangle, CheckCircle2, KeyRound, Wifi } from "lucide-react";
import { onlineCallStats, onlineKeyStatus, onlineSetKey, onlineTest } from "../core/online";

interface Props {
  values: Record<string, string>;
  onSettingChange: (key: string, value: string) => void;
  onToast: (msg: string) => void;
}

const SOURCES = ["musicbrainz", "coverartarchive", "acoustid", "deezer", "itunes"] as const;
const DEFAULT_ON: Record<string, boolean> = { musicbrainz: true, coverartarchive: true, acoustid: true, deezer: false, itunes: false };

/** Réglages › Recherche en ligne. Les clés vont dans le trousseau du système. */
export function OnlineSettings({ values, onSettingChange, onToast }: Props) {
  const { t } = useTranslation();
  const [keys, setKeys] = useState({ client: false, user: false });
  const [client, setClient] = useState("");
  const [user, setUser] = useState("");
  const [contact, setContact] = useState(values.online_contact ?? "");
  const [test, setTest] = useState<{ musicbrainz: string; acoustid: string } | null>(null);
  const [testing, setTesting] = useState(false);
  const [stats, setStats] = useState<[string, number, number][]>([]);

  useEffect(() => {
    void onlineKeyStatus().then(setKeys);
    void onlineCallStats().then(setStats);
  }, []);
  useEffect(() => setContact(values.online_contact ?? ""), [values.online_contact]);

  const on = (s: string) => (values[`src_${s}`] ?? String(DEFAULT_ON[s])) === "true";

  const saveKey = async (name: "acoustid_client" | "acoustid_user", v: string) => {
    try {
      await onlineSetKey(name, v);
      setKeys(await onlineKeyStatus());
      onToast(v.trim() ? t("onlineSettings.keySaved") : t("onlineSettings.keyRemoved"));
      if (name === "acoustid_client") setClient("");
      else setUser("");
    } catch {
      onToast(t("onlineSettings.keyError"));
    }
  };

  return (
    <section>
      <h2>{t("onlineSettings.title")}</h2>
      <p className="hint">{t("onlineSettings.hint")}</p>

      <h3 className="form-label">{t("onlineSettings.sources")}</h3>
      <div className="checks">
        {SOURCES.map((s) => (
          <label key={s}>
            <input type="checkbox" checked={on(s)} onChange={(e) => onSettingChange(`src_${s}`, String(e.target.checked))} />
            <span>
              <strong>{t(`sources.${s}`)}</strong> — {t(`onlineSettings.src.${s}`)}
              {(s === "deezer" || s === "itunes") && (
                <span className="text-warning small block">
                  <AlertTriangle size={12} aria-hidden /> {t(`onlineSettings.warn.${s}`)}
                </span>
              )}
            </span>
          </label>
        ))}
      </div>

      <h3 className="form-label"><KeyRound size={14} aria-hidden /> {t("onlineSettings.acoustid")}</h3>
      <ol className="steps small">
        <li>{t("onlineSettings.step1")} <span className="mono">acoustid.org/new-application</span></li>
        <li>{t("onlineSettings.step2")} <span className="mono">acoustid.org/api-key</span></li>
        <li>{t("onlineSettings.step3")}</li>
      </ol>
      <div className="key-row">
        <span className="key-label">{t("onlineSettings.clientKey")}</span>
        <input className="text-input" type="password" autoComplete="off" value={client} placeholder={keys.client ? t("onlineSettings.keyPresent") : ""} onChange={(e) => setClient(e.target.value)} />
        <button className="btn" disabled={!client.trim()} onClick={() => void saveKey("acoustid_client", client)}>{t("rename.save")}</button>
        {keys.client && <button className="btn" onClick={() => void saveKey("acoustid_client", "")}>{t("onlineSettings.remove")}</button>}
      </div>
      <div className="key-row">
        <span className="key-label">{t("onlineSettings.userKey")}</span>
        <input className="text-input" type="password" autoComplete="off" value={user} placeholder={keys.user ? t("onlineSettings.keyPresent") : ""} onChange={(e) => setUser(e.target.value)} />
        <button className="btn" disabled={!user.trim()} onClick={() => void saveKey("acoustid_user", user)}>{t("rename.save")}</button>
        {keys.user && <button className="btn" onClick={() => void saveKey("acoustid_user", "")}>{t("onlineSettings.remove")}</button>}
      </div>
      <p className="hint small">{t("onlineSettings.keyHint")}</p>

      <h3 className="form-label">{t("onlineSettings.contact")}</h3>
      <p className="hint small">{t("onlineSettings.contactHint")}</p>
      <div className="key-row">
        <input className="text-input" style={{ flex: 1 }} value={contact} placeholder="https://… ou adresse e-mail" onChange={(e) => setContact(e.target.value)} />
        <button className="btn" disabled={contact === (values.online_contact ?? "")} onClick={() => onSettingChange("online_contact", contact.trim())}>{t("rename.save")}</button>
      </div>

      <h3 className="form-label">{t("onlineSettings.thresholds")}</h3>
      <div className="key-row">
        <label className="chip-select">
          {t("onlineSettings.auto")}
          <select value={values.auto_threshold ?? "90"} onChange={(e) => onSettingChange("auto_threshold", e.target.value)}>
            {["70", "75", "80", "85", "90", "95", "100"].map((v) => <option key={v} value={v}>{v}</option>)}
          </select>
        </label>
        <label className="chip-select">
          {t("onlineSettings.review")}
          <select value={values.review_threshold ?? "60"} onChange={(e) => onSettingChange("review_threshold", e.target.value)}>
            {["40", "45", "50", "55", "60", "65", "70"].map((v) => <option key={v} value={v}>{v}</option>)}
          </select>
        </label>
      </div>
      <div className="checks">
        <label>
          <input type="checkbox" checked={values.replace_existing === "true"} onChange={(e) => onSettingChange("replace_existing", String(e.target.checked))} />
          {t("onlineSettings.replaceExisting")}
        </label>
      </div>
      <h3 className="form-label">{t("onlineSettings.autoRename")}</h3>
      <select className="text-input" value={values.auto_rename ?? "inbox"} onChange={(e) => onSettingChange("auto_rename", e.target.value)}>
        {["inbox", "library", "never"].map((v) => <option key={v} value={v}>{t(`onlineSettings.rename.${v}`)}</option>)}
      </select>

      <div className="folder-actions" style={{ marginTop: 16 }}>
        <button
          className="btn"
          disabled={testing}
          onClick={async () => {
            setTesting(true);
            try {
              setTest(await onlineTest());
              setStats(await onlineCallStats());
            } finally {
              setTesting(false);
            }
          }}
        >
          <Wifi size={16} aria-hidden /> {t("onlineSettings.test")}
        </button>
      </div>
      {test && (
        <ul className="test-results small">
          {(["musicbrainz", "acoustid"] as const).map((k) => (
            <li key={k}>
              {test[k] === "ok" ? <CheckCircle2 size={14} className="text-success" aria-hidden /> : <AlertTriangle size={14} className="text-warning" aria-hidden />}{" "}
              {t(`sources.${k}`)} : {t(`onlineSettings.result.${test[k]}`, { defaultValue: test[k] })}
            </li>
          ))}
        </ul>
      )}
      {stats.some(([, total]) => total > 0) && (
        <>
          <h3 className="form-label">{t("onlineSettings.calls")}</h3>
          <table className="calls-table small tabular">
            <tbody>
              {stats.map(([src, total, max]) => (
                <tr key={src}>
                  <td>{t(`sources.${src}`)}</td>
                  <td>{t("onlineSettings.callsTotal", { count: total })}</td>
                  <td>{t(`onlineSettings.callsMax.${src}`, { count: max })}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </>
      )}
    </section>
  );
}
