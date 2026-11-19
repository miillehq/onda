import { Validator, type Schema } from "@cfworker/json-schema";
import contract from "../../../contracts/desktop-request.schema.json";
import type { AppError } from "../generated/core";

const branches = new Map(contract.oneOf.map((branch) => [branch.properties.command.const, branch]));
const validators = new Map<string, Validator>();

/** Validate against the Rust-generated branch for this command before mutating the mock. */
export function requireRequestShape(request: unknown): void {
  const invalid = (): AppError => ({ code: "invalidInput", message: "The desktop request has an invalid shape.", retryable: false });
  if (!request || typeof request !== "object" || Array.isArray(request) || !("command" in request) || typeof request.command !== "string") {
    throw invalid();
  }
  const command = request.command;
  const branch = branches.get(command);
  if (!branch) throw invalid();
  let validator = validators.get(command);
  if (!validator) {
    const schema = { ...branch, definitions: contract.definitions } as unknown as Schema;
    validator = new Validator(schema, "7", true);
    validators.set(command, validator);
  }
  if (!validator.validate(request).valid) throw invalid();
}
