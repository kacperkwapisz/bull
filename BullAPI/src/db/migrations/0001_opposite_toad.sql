CREATE TABLE "daily_energy" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"user_id" uuid NOT NULL,
	"source_bundle_id" uuid,
	"day" date NOT NULL,
	"energy_score" double precision,
	"energy_bank" double precision,
	"charge_rate" double precision,
	"drain_rate" double precision,
	"source" text,
	"raw" jsonb,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE TABLE "daily_strain" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"user_id" uuid NOT NULL,
	"source_bundle_id" uuid,
	"day" date NOT NULL,
	"strain_score" double precision,
	"kilojoules" double precision,
	"avg_hr_bpm" double precision,
	"max_hr_bpm" double precision,
	"source" text,
	"raw" jsonb,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE TABLE "daily_stress" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"user_id" uuid NOT NULL,
	"source_bundle_id" uuid,
	"day" date NOT NULL,
	"stress_score" double precision,
	"avg_stress" double precision,
	"max_stress" double precision,
	"high_stress_minutes" double precision,
	"source" text,
	"raw" jsonb,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE TABLE "vitals_daily" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"user_id" uuid NOT NULL,
	"source_bundle_id" uuid,
	"day" date NOT NULL,
	"resting_hr_bpm" double precision,
	"hrv_ms" double precision,
	"respiratory_rate" double precision,
	"skin_temp_c" double precision,
	"spo2_pct" double precision,
	"source" text,
	"raw" jsonb,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
ALTER TABLE "daily_energy" ADD CONSTRAINT "daily_energy_user_id_users_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "daily_energy" ADD CONSTRAINT "daily_energy_source_bundle_id_upload_bundles_id_fk" FOREIGN KEY ("source_bundle_id") REFERENCES "public"."upload_bundles"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "daily_strain" ADD CONSTRAINT "daily_strain_user_id_users_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "daily_strain" ADD CONSTRAINT "daily_strain_source_bundle_id_upload_bundles_id_fk" FOREIGN KEY ("source_bundle_id") REFERENCES "public"."upload_bundles"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "daily_stress" ADD CONSTRAINT "daily_stress_user_id_users_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "daily_stress" ADD CONSTRAINT "daily_stress_source_bundle_id_upload_bundles_id_fk" FOREIGN KEY ("source_bundle_id") REFERENCES "public"."upload_bundles"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "vitals_daily" ADD CONSTRAINT "vitals_daily_user_id_users_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "vitals_daily" ADD CONSTRAINT "vitals_daily_source_bundle_id_upload_bundles_id_fk" FOREIGN KEY ("source_bundle_id") REFERENCES "public"."upload_bundles"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
CREATE UNIQUE INDEX "daily_energy_user_day_uq" ON "daily_energy" USING btree ("user_id","day");--> statement-breakpoint
CREATE UNIQUE INDEX "daily_strain_user_day_uq" ON "daily_strain" USING btree ("user_id","day");--> statement-breakpoint
CREATE UNIQUE INDEX "daily_stress_user_day_uq" ON "daily_stress" USING btree ("user_id","day");--> statement-breakpoint
CREATE UNIQUE INDEX "vitals_daily_user_day_uq" ON "vitals_daily" USING btree ("user_id","day");