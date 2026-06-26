CREATE TABLE "sync_runs" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"user_id" uuid NOT NULL,
	"device_id" text,
	"upload_bundle_id" uuid,
	"source" text DEFAULT 'unknown' NOT NULL,
	"trigger_timestamp" timestamp with time zone DEFAULT now() NOT NULL,
	"result_timestamp" timestamp with time zone DEFAULT now() NOT NULL,
	"total_packet_upload" integer DEFAULT 0 NOT NULL,
	"upload_retry_count" integer DEFAULT 0 NOT NULL,
	"status" text DEFAULT 'uploaded' NOT NULL
);
--> statement-breakpoint
ALTER TABLE "sync_runs" ADD CONSTRAINT "sync_runs_user_id_users_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "sync_runs" ADD CONSTRAINT "sync_runs_upload_bundle_id_upload_bundles_id_fk" FOREIGN KEY ("upload_bundle_id") REFERENCES "public"."upload_bundles"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
CREATE INDEX "sync_runs_user_result_idx" ON "sync_runs" USING btree ("user_id","result_timestamp");