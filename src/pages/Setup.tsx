import { useRef, useState, type ReactNode } from "react";
import { useNavigate, Navigate } from "react-router";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import clsx from "clsx";
import { ArrowLeft, ArrowRight, BadgeCheck, Building2, Check, CircleSlash, FileText, Loader2 } from "lucide-react";
import { api, ApiError, keys, type SetupBusinessInput } from "../lib/api";
import { CONSTITUTIONS, MONTHS, STATES, constitutionFromPan, gstinError, gstinParts, isValidPan, stateName } from "../lib/gst";
import { Button, Card } from "../components/ui";
import { ChoiceCards, Field, FormGrid, Select, TextInput, Toggle } from "../components/form";
import { useToast } from "../components/Toast";
import { ThemeSwitcher } from "../components/ThemeSwitcher";
import { BrandMark, Wordmark } from "../components/Brand";

const STEPS = ["GST registration", "Business details", "Books & numbering", "Review"] as const;

interface Draft {
  registered: "yes" | "no";
  gstin: string;
  registrationType: string;
  filingFrequency: "monthly" | "quarterly";
  effectiveFrom: string;
  legalName: string;
  tradeName: string;
  constitution: string;
  pan: string;
  addressLine1: string;
  addressLine2: string;
  city: string;
  pincode: string;
  stateCode: number;
  email: string;
  phone: string;
  fyStartMonth: number;
  booksStartDate: string;
  roundToRupee: boolean;
  invoicePrefix: string;
  signatoryName: string;
}

type Errors = Partial<Record<keyof Draft, string>>;

/** Which step owns each field — used to jump to the first server error. */
const STEP_OF: Record<keyof Draft, number> = {
  registered: 0, gstin: 0, registrationType: 0, filingFrequency: 0, effectiveFrom: 0,
  legalName: 1, tradeName: 1, constitution: 1, pan: 1, addressLine1: 1, addressLine2: 1, city: 1, pincode: 1,
  stateCode: 1, email: 1, phone: 1,
  fyStartMonth: 2, booksStartDate: 2, roundToRupee: 2, invoicePrefix: 2, signatoryName: 2,
};

function validateStep(step: number, d: Draft): Errors {
  const e: Errors = {};
  if (step === 0 && d.registered === "yes") {
    const g = gstinError(d.gstin);
    if (g) e.gstin = g;
    if (!d.effectiveFrom) e.effectiveFrom = "Enter the date your registration took effect.";
  }
  if (step === 1) {
    if (!d.legalName.trim()) e.legalName = "Enter the legal name of the business.";
    if (!d.constitution) e.constitution = "Choose the type of business.";
    if (d.pan && !isValidPan(d.pan)) e.pan = "PAN must be 5 letters, 4 digits and 1 letter.";
    if (d.pincode && !/^[1-9][0-9]{5}$/.test(d.pincode)) e.pincode = "PIN code must be 6 digits.";
    if (!d.stateCode) e.stateCode = "Choose a state.";
  }
  if (step === 2) {
    if (!d.booksStartDate) e.booksStartDate = "Choose the date your books start.";
    if (!/^[A-Za-z0-9]{1,5}$/.test(d.invoicePrefix)) e.invoicePrefix = "Use up to 5 letters or digits.";
  }
  return e;
}

export function Setup() {
  const navigate = useNavigate();
  const toast = useToast();
  const qc = useQueryClient();
  const { data: profile, isLoading } = useQuery({ queryKey: keys.profile, queryFn: api.businessProfile });

  const fy = profile?.currentFinancialYear;
  const [step, setStep] = useState(0);
  const direction = useRef<"forward" | "back">("forward");
  const [errors, setErrors] = useState<Errors>({});
  const [formError, setFormError] = useState<string | null>(null);
  const [d, setD] = useState<Draft>({
    registered: "yes",
    gstin: "",
    registrationType: "regular",
    filingFrequency: "monthly",
    effectiveFrom: "",
    legalName: "",
    tradeName: "",
    constitution: "",
    pan: "",
    addressLine1: "",
    addressLine2: "",
    city: "",
    pincode: "",
    stateCode: 0,
    email: "",
    phone: "",
    fyStartMonth: 4,
    booksStartDate: "",
    roundToRupee: true,
    invoicePrefix: "INV",
    signatoryName: "",
  });

  const set = <K extends keyof Draft>(k: K, v: Draft[K]) => {
    setD((prev) => ({ ...prev, [k]: v }));
    if (errors[k]) setErrors((e) => ({ ...e, [k]: undefined }));
  };

  const parts = d.registered === "yes" ? gstinParts(d.gstin) : null;
  const shortFy = fy ? `${fy.code.slice(2, 4)}-${fy.code.slice(5)}` : "";
  const preview = `${(d.invoicePrefix || "INV").toUpperCase()}/${shortFy}/0001`;

  const go = (to: number) => {
    direction.current = to > step ? "forward" : "back";
    setStep(to);
  };

  const next = () => {
    const e = validateStep(step, d);
    setErrors(e);
    if (Object.keys(e).length) return;
    if (step === 0 && parts) {
      // The GSTIN carries the state and PAN: prefill them (and a likely constitution).
      setD((p) => ({
        ...p,
        pan: parts.pan,
        stateCode: parts.stateCode,
        constitution: p.constitution || constitutionFromPan(parts.pan) || "",
      }));
    }
    if (step === 1 && !d.booksStartDate && fy) set("booksStartDate", fy.startDate);
    go(step + 1);
  };

  const create = useMutation({
    mutationFn: () => {
      const input: SetupBusinessInput = {
        organisation: {
          legalName: d.legalName,
          tradeName: d.tradeName,
          pan: d.pan,
          constitution: d.constitution,
          addressLine1: d.addressLine1,
          addressLine2: d.addressLine2,
          city: d.city,
          pincode: d.pincode,
          stateCode: d.stateCode,
          email: d.email,
          phone: d.phone,
          fyStartMonth: d.fyStartMonth,
          booksStartDate: d.booksStartDate,
          roundToRupee: d.roundToRupee,
        },
        registration:
          d.registered === "yes"
            ? {
                gstin: d.gstin,
                registrationType: d.registrationType,
                filingFrequency: d.filingFrequency,
                effectiveFrom: d.effectiveFrom,
                signatoryName: d.signatoryName,
              }
            : undefined,
        invoicePrefix: d.invoicePrefix,
      };
      return api.setupBusiness(input);
    },
    onSuccess: (p) => {
      qc.setQueryData(keys.profile, p);
      void qc.invalidateQueries();
      toast(`${p.organisation?.tradeName || p.organisation?.legalName} is ready`);
      navigate("/", { replace: true });
    },
    onError: (err) => {
      if (!(err instanceof ApiError)) return setFormError(String(err));
      const mapped: Errors = {};
      for (const [path, msg] of Object.entries(err.fields)) {
        const key = path.split(".").pop() as keyof Draft;
        if (key in STEP_OF) mapped[key] = msg;
      }
      setErrors(mapped);
      const steps = Object.keys(mapped).map((k) => STEP_OF[k as keyof Draft]);
      if (steps.length) go(Math.min(...steps));
      setFormError(steps.length ? null : err.message);
    },
  });

  const content =
    step === 0 ? (
      <GstStep d={d} set={set} errors={errors} parts={parts} />
    ) : step === 1 ? (
      <BusinessStep d={d} set={set} errors={errors} />
    ) : step === 2 ? (
      <BooksStep d={d} set={set} errors={errors} preview={preview} fyLabel={fy?.label ?? ""} />
    ) : (
      <ReviewStep d={d} preview={preview} onEdit={go} />
    );

  if (!isLoading && profile?.organisation) return <Navigate to="/" replace />;

  return (
    <div className="h-full overflow-y-auto">
      <div className="mx-auto flex min-h-full w-full max-w-[680px] flex-col px-6 pb-10 pt-8">
        <header className="mb-7 flex items-center justify-between">
          <div className="flex items-center gap-2.5">
            <BrandMark size={32} />
            <Wordmark className="text-[17px]" />
          </div>
          <ThemeSwitcher />
        </header>

        <h1 className="text-display">Set up your business</h1>
        <p className="mt-1.5 text-[14px] text-muted">
          Four short steps. Everything stays on this computer, and you can change it later in Settings.
        </p>

        <Stepper step={step} onJump={(i) => i < step && go(i)} />

        <Card className="mt-5 overflow-hidden">
          <div key={step} className={clsx("p-6", direction.current === "forward" ? "step-forward" : "step-back")}>
            <h2 className="text-title mb-1">{STEPS[step]}</h2>
            {content}
          </div>
          {formError && (
            <p className="mx-6 mb-4 rounded-lg bg-danger-soft px-3 py-2 text-[13px] text-danger" role="alert">
              {formError}
            </p>
          )}
          <footer className="flex items-center justify-between border-t border-line bg-surface-2 px-6 py-3.5">
            {step > 0 ? (
              <Button variant="ghost" onClick={() => go(step - 1)}>
                <ArrowLeft size={15} /> Back
              </Button>
            ) : (
              <Button variant="ghost" onClick={() => navigate("/")}>
                Later
              </Button>
            )}
            {step < STEPS.length - 1 ? (
              <Button variant="primary" onClick={next}>
                Continue <ArrowRight size={15} />
              </Button>
            ) : (
              <Button variant="primary" onClick={() => create.mutate()} disabled={create.isPending}>
                {create.isPending ? <Loader2 size={15} className="animate-spin" /> : <Check size={15} />}
                Create my books
              </Button>
            )}
          </footer>
        </Card>
      </div>
    </div>
  );
}

function Stepper({ step, onJump }: { step: number; onJump: (i: number) => void }) {
  return (
    <ol className="mt-7 grid grid-cols-4 gap-2" aria-label="Setup progress">
      {STEPS.map((label, i) => {
        const done = i < step;
        const current = i === step;
        return (
          <li key={label}>
            <button
              type="button"
              onClick={() => onJump(i)}
              disabled={!done}
              aria-current={current ? "step" : undefined}
              className="group flex w-full flex-col gap-2 text-left disabled:cursor-default"
            >
              <span className="h-1 overflow-hidden rounded-full bg-active">
                <span
                  className="block h-full origin-left rounded-full bg-accent-fill transition-transform duration-300 ease-out-strong"
                  style={{ transform: `scaleX(${done || current ? 1 : 0})` }}
                />
              </span>
              <span
                className={clsx(
                  "flex items-center gap-1.5 text-[12px] font-medium",
                  current ? "text-fg" : done ? "text-accent group-hover:underline" : "text-muted",
                )}
              >
                {done && <Check size={12} strokeWidth={3} />}
                {label}
              </span>
            </button>
          </li>
        );
      })}
    </ol>
  );
}

interface StepProps {
  d: Draft;
  set: <K extends keyof Draft>(k: K, v: Draft[K]) => void;
  errors: Errors;
}

function GstStep({ d, set, errors, parts }: StepProps & { parts: { stateCode: number; pan: string } | null }) {
  return (
    <div className="space-y-5">
      <p className="text-[13px] text-muted">Your GSTIN fills in your state and PAN automatically.</p>
      <ChoiceCards
        name="GST registration"
        value={d.registered}
        onChange={(v) => set("registered", v)}
        options={[
          { value: "yes", title: "Registered under GST", description: "I have a GSTIN", icon: <BadgeCheck size={18} /> },
          { value: "no", title: "Not registered yet", description: "Add a GSTIN later in Settings", icon: <CircleSlash size={18} /> },
        ]}
      />
      {d.registered === "yes" ? (
        <>
          <Field
            label="GSTIN"
            error={errors.gstin}
            hint={
              parts ? (
                <span className="flex items-center gap-1.5 text-success">
                  <Check size={13} strokeWidth={3} /> {stateName(parts.stateCode)} · PAN {parts.pan}
                </span>
              ) : (
                "15 characters, e.g. 27ABCDE1234F1Z5"
              )
            }
          >
            {(p) => (
              <TextInput
                id={p.id}
                invalid={p.invalid}
                aria-describedby={p.describedBy}
                value={d.gstin}
                onChange={(e) => set("gstin", e.target.value.toUpperCase().replace(/\s/g, ""))}
                maxLength={15}
                autoFocus
                spellCheck={false}
                className="num font-medium tracking-[0.04em]"
                placeholder="27ABCDE1234F1Z5"
              />
            )}
          </Field>
          <div>
            <div className="mb-1.5 text-[12.5px] font-medium">Return filing</div>
            <ChoiceCards
              name="Filing frequency"
              value={d.filingFrequency}
              onChange={(v) => set("filingFrequency", v)}
              options={[
                { value: "monthly", title: "Monthly", description: "GSTR-1 and GSTR-3B every month" },
                { value: "quarterly", title: "Quarterly (QRMP)", description: "Quarterly returns, monthly tax payment" },
              ]}
            />
          </div>
          <FormGrid>
            <Field label="Registration type" error={errors.registrationType}>
              {(p) => (
                <Select id={p.id} invalid={p.invalid} value={d.registrationType} onChange={(e) => set("registrationType", e.target.value)}>
                  <option value="regular">Regular</option>
                  <option value="sez_unit">SEZ unit</option>
                  <option value="casual">Casual taxable person</option>
                </Select>
              )}
            </Field>
            <Field label="Registered from" error={errors.effectiveFrom} hint="As on your registration certificate">
              {(p) => (
                <TextInput
                  id={p.id}
                  type="date"
                  invalid={p.invalid}
                  aria-describedby={p.describedBy}
                  value={d.effectiveFrom}
                  onChange={(e) => set("effectiveFrom", e.target.value)}
                />
              )}
            </Field>
          </FormGrid>
        </>
      ) : (
        <p className="rounded-xl bg-surface-2 p-3.5 text-[13px] text-muted">
          You'll issue bills of supply, without GST. When you register, add your GSTIN in Settings and tax invoices
          become available.
        </p>
      )}
    </div>
  );
}

function BusinessStep({ d, set, errors }: StepProps) {
  const fromGstin = d.registered === "yes";
  const text = (k: keyof Draft, label: string, opts: { optional?: boolean; placeholder?: string; hint?: string; className?: string; type?: string; transform?: (s: string) => string } = {}) => (
    <Field label={label} error={errors[k]} optional={opts.optional} hint={opts.hint} className={opts.className}>
      {(p) => (
        <TextInput
          id={p.id}
          type={opts.type ?? "text"}
          invalid={p.invalid}
          aria-describedby={p.describedBy}
          value={String(d[k] ?? "")}
          placeholder={opts.placeholder}
          onChange={(e) => set(k, (opts.transform ? opts.transform(e.target.value) : e.target.value) as never)}
        />
      )}
    </Field>
  );

  return (
    <div className="space-y-4 pt-3">
      <FormGrid>
        {text("legalName", "Legal name", { placeholder: "As on your PAN / GST certificate", className: "col-span-2" })}
        {text("tradeName", "Trade name", { optional: true, placeholder: "Shown on invoices if different" })}
        <Field label="Type of business" error={errors.constitution}>
          {(p) => (
            <Select id={p.id} invalid={p.invalid} value={d.constitution} onChange={(e) => set("constitution", e.target.value)}>
              <option value="" disabled>
                Choose…
              </option>
              {CONSTITUTIONS.map((c) => (
                <option key={c.value} value={c.value}>
                  {c.label}
                </option>
              ))}
            </Select>
          )}
        </Field>
        <Field label="PAN" error={errors.pan} optional={!fromGstin} hint={fromGstin ? "From your GSTIN" : undefined}>
          {(p) => (
            <TextInput
              id={p.id}
              invalid={p.invalid}
              aria-describedby={p.describedBy}
              value={d.pan}
              disabled={fromGstin}
              maxLength={10}
              className="num tracking-[0.04em]"
              onChange={(e) => set("pan", e.target.value.toUpperCase())}
            />
          )}
        </Field>
        <Field label="State" error={errors.stateCode} hint={fromGstin ? "From your GSTIN" : undefined}>
          {(p) => (
            <Select
              id={p.id}
              invalid={p.invalid}
              value={d.stateCode || ""}
              disabled={fromGstin}
              onChange={(e) => set("stateCode", Number(e.target.value))}
            >
              <option value="" disabled>
                Choose…
              </option>
              {STATES.map((s) => (
                <option key={s.code} value={s.code}>
                  {String(s.code).padStart(2, "0")} · {s.name}
                </option>
              ))}
            </Select>
          )}
        </Field>
        {text("addressLine1", "Address", { optional: true, placeholder: "Building, street", className: "col-span-2" })}
        {text("addressLine2", "Address line 2", { optional: true, placeholder: "Area, landmark", className: "col-span-2" })}
        {text("city", "City", { optional: true })}
        {text("pincode", "PIN code", { optional: true, transform: (s) => s.replace(/\D/g, "").slice(0, 6) })}
        {text("email", "E-mail", { optional: true, type: "email" })}
        {text("phone", "Phone", { optional: true, type: "tel" })}
      </FormGrid>
    </div>
  );
}

function BooksStep({ d, set, errors, preview, fyLabel }: StepProps & { preview: string; fyLabel: string }) {
  return (
    <div className="space-y-5 pt-3">
      <FormGrid>
        <Field label="Books start on" error={errors.booksStartDate} hint="Opening balances are entered as on this date">
          {(p) => (
            <TextInput
              id={p.id}
              type="date"
              invalid={p.invalid}
              aria-describedby={p.describedBy}
              value={d.booksStartDate}
              onChange={(e) => set("booksStartDate", e.target.value)}
            />
          )}
        </Field>
        <Field label="Financial year starts in" error={errors.fyStartMonth} hint="April for almost every Indian business">
          {(p) => (
            <Select id={p.id} invalid={p.invalid} value={d.fyStartMonth} onChange={(e) => set("fyStartMonth", Number(e.target.value))}>
              {MONTHS.map((m, i) => (
                <option key={m} value={i + 1}>
                  {m}
                </option>
              ))}
            </Select>
          )}
        </Field>
      </FormGrid>

      <div className="rounded-xl border border-line p-4">
        <div className="flex items-start gap-4">
          <Field
            label={d.registered === "yes" ? "Invoice prefix" : "Bill prefix"}
            error={errors.invoicePrefix}
            className="w-[140px] shrink-0"
          >
            {(p) => (
              <TextInput
                id={p.id}
                invalid={p.invalid}
                aria-describedby={p.describedBy}
                value={d.invoicePrefix}
                maxLength={5}
                className="num font-medium uppercase"
                onChange={(e) => set("invoicePrefix", e.target.value.toUpperCase().replace(/[^A-Z0-9]/g, ""))}
              />
            )}
          </Field>
          <div className="min-w-0 flex-1">
            <div className="text-[12.5px] font-medium">First number</div>
            <div className="mt-1.5 flex h-9 items-center gap-2 rounded-[9px] bg-surface-2 px-3">
              <FileText size={15} className="text-subtle" aria-hidden />
              <span className="num text-[14px] font-semibold tracking-[0.02em]">{preview}</span>
            </div>
            <p className="mt-1.5 text-[12px] text-muted">
              Numbers restart each financial year ({fyLabel}). GST allows up to 16 characters.
            </p>
          </div>
        </div>
      </div>

      <Toggle
        checked={d.roundToRupee}
        onChange={(v) => set("roundToRupee", v)}
        label="Round invoice totals to the nearest rupee"
        description="The difference is posted to a Round-off account."
      />

      {d.registered === "yes" && (
        <Field label="Authorised signatory" optional error={errors.signatoryName} hint="Printed on invoices and used when approving returns">
          {(p) => (
            <TextInput id={p.id} value={d.signatoryName} onChange={(e) => set("signatoryName", e.target.value)} />
          )}
        </Field>
      )}
    </div>
  );
}

function ReviewStep({ d, preview, onEdit }: { d: Draft; preview: string; onEdit: (step: number) => void }) {
  const constitution = CONSTITUTIONS.find((c) => c.value === d.constitution)?.label ?? "—";
  const address = [d.addressLine1, d.addressLine2, d.city, d.pincode].filter(Boolean).join(", ");
  return (
    <div className="space-y-4 pt-3">
      <ReviewGroup title="GST registration" onEdit={() => onEdit(0)}>
        {d.registered === "yes" ? (
          <>
            <ReviewRow label="GSTIN" value={<span className="num">{d.gstin}</span>} />
            <ReviewRow label="Filing" value={d.filingFrequency === "monthly" ? "Monthly" : "Quarterly (QRMP)"} />
            <ReviewRow label="Registered from" value={d.effectiveFrom} />
          </>
        ) : (
          <ReviewRow label="Status" value="Not registered" />
        )}
      </ReviewGroup>
      <ReviewGroup title="Business" onEdit={() => onEdit(1)}>
        <ReviewRow label="Legal name" value={d.legalName} />
        {d.tradeName && <ReviewRow label="Trade name" value={d.tradeName} />}
        <ReviewRow label="Type" value={constitution} />
        <ReviewRow label="PAN" value={<span className="num">{d.pan || "—"}</span>} />
        <ReviewRow label="State" value={stateName(d.stateCode)} />
        {address && <ReviewRow label="Address" value={address} />}
      </ReviewGroup>
      <ReviewGroup title="Books" onEdit={() => onEdit(2)}>
        <ReviewRow label="Books start" value={d.booksStartDate} />
        <ReviewRow label="First number" value={<span className="num">{preview}</span>} />
        <ReviewRow label="Round-off" value={d.roundToRupee ? "Nearest rupee" : "Off"} />
      </ReviewGroup>
      <p className="flex gap-2 text-[12.5px] text-muted">
        <Building2 size={15} className="mt-px shrink-0 text-subtle" aria-hidden />
        We'll also create a standard chart of accounts with GST ledgers, which you can rename later.
      </p>
    </div>
  );
}

function ReviewGroup({ title, onEdit, children }: { title: string; onEdit: () => void; children: ReactNode }) {
  return (
    <section className="rounded-xl border border-line">
      <div className="flex items-center justify-between border-b border-line px-4 py-2.5">
        <h3 className="text-[13px] font-semibold">{title}</h3>
        <button type="button" onClick={onEdit} className="text-[12.5px] font-medium text-accent hover:underline">
          Edit
        </button>
      </div>
      <dl className="px-4 py-1.5">{children}</dl>
    </section>
  );
}

function ReviewRow({ label, value }: { label: string; value: ReactNode }) {
  return (
    <div className="flex justify-between gap-6 py-1.5 text-[13px]">
      <dt className="text-muted">{label}</dt>
      <dd className="text-right font-medium">{value}</dd>
    </div>
  );
}
