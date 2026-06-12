CREATE TABLE "apple_identities" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"user_id" uuid NOT NULL,
	"apple_sub" text NOT NULL,
	"email" text,
	"is_private_email" integer DEFAULT 0 NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE TABLE "daily_recovery" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"user_id" uuid NOT NULL,
	"source_bundle_id" uuid,
	"day" date NOT NULL,
	"recovery_score" double precision,
	"hrv_ms" double precision,
	"resting_hr_bpm" double precision,
	"raw" jsonb,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE TABLE "daily_sleep" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"user_id" uuid NOT NULL,
	"source_bundle_id" uuid,
	"day" date NOT NULL,
	"sleep_score" double precision,
	"total_sleep_minutes" double precision,
	"rem_minutes" double precision,
	"deep_minutes" double precision,
	"light_minutes" double precision,
	"awake_minutes" double precision,
	"raw" jsonb,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE TABLE "devices" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"user_id" uuid NOT NULL,
	"device_id" text NOT NULL,
	"platform" text DEFAULT 'ios' NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"last_seen_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE TABLE "spo2_samples" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"user_id" uuid NOT NULL,
	"source_bundle_id" uuid,
	"recorded_at" timestamp with time zone NOT NULL,
	"spo2" double precision,
	"raw" jsonb
);
--> statement-breakpoint
CREATE TABLE "upload_bundles" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"user_id" uuid NOT NULL,
	"device_id" text,
	"checksum" text NOT NULL,
	"byte_size" bigint NOT NULL,
	"status" text DEFAULT 'pending' NOT NULL,
	"storage_key" text NOT NULL,
	"content_type" text DEFAULT 'application/octet-stream' NOT NULL,
	"timeframe_start" timestamp with time zone,
	"timeframe_end" timestamp with time zone,
	"parse_error" text,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"parsed_at" timestamp with time zone
);
--> statement-breakpoint
CREATE TABLE "users" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"last_seen_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
ALTER TABLE "apple_identities" ADD CONSTRAINT "apple_identities_user_id_users_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "daily_recovery" ADD CONSTRAINT "daily_recovery_user_id_users_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "daily_recovery" ADD CONSTRAINT "daily_recovery_source_bundle_id_upload_bundles_id_fk" FOREIGN KEY ("source_bundle_id") REFERENCES "public"."upload_bundles"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "daily_sleep" ADD CONSTRAINT "daily_sleep_user_id_users_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "daily_sleep" ADD CONSTRAINT "daily_sleep_source_bundle_id_upload_bundles_id_fk" FOREIGN KEY ("source_bundle_id") REFERENCES "public"."upload_bundles"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "devices" ADD CONSTRAINT "devices_user_id_users_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "spo2_samples" ADD CONSTRAINT "spo2_samples_user_id_users_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "spo2_samples" ADD CONSTRAINT "spo2_samples_source_bundle_id_upload_bundles_id_fk" FOREIGN KEY ("source_bundle_id") REFERENCES "public"."upload_bundles"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "upload_bundles" ADD CONSTRAINT "upload_bundles_user_id_users_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
CREATE UNIQUE INDEX "apple_identities_apple_sub_uq" ON "apple_identities" USING btree ("apple_sub");--> statement-breakpoint
CREATE UNIQUE INDEX "daily_recovery_user_day_uq" ON "daily_recovery" USING btree ("user_id","day");--> statement-breakpoint
CREATE UNIQUE INDEX "daily_sleep_user_day_uq" ON "daily_sleep" USING btree ("user_id","day");--> statement-breakpoint
CREATE UNIQUE INDEX "devices_user_device_uq" ON "devices" USING btree ("user_id","device_id");--> statement-breakpoint
CREATE UNIQUE INDEX "spo2_samples_user_ts_uq" ON "spo2_samples" USING btree ("user_id","recorded_at");--> statement-breakpoint
CREATE INDEX "spo2_samples_user_ts_idx" ON "spo2_samples" USING btree ("user_id","recorded_at");--> statement-breakpoint
CREATE UNIQUE INDEX "upload_bundles_user_checksum_uq" ON "upload_bundles" USING btree ("user_id","checksum");--> statement-breakpoint
CREATE INDEX "upload_bundles_user_created_idx" ON "upload_bundles" USING btree ("user_id","created_at");