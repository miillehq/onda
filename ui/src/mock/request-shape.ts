import { Validator, type Schema } from "@cfworker/json-schema";
import contract from "../../../contracts/desktop-request.schema.json";
import type { ApiRequest } from "../generated/api";
import type { AppError } from "../generated/core";

const branches = new Map(contract.oneOf.map((branch) => [branch.properties.command.const, branch]));
const validators = new Map<string, Validator>();

/** What the desktop bridge carries: JSON, which drops `undefined` members and cannot encode everything. */
function wireForm(received: unknown): unknown {
  try {
    const encoded = JSON.stringify(received);
    return encoded === undefined ? undefined : JSON.parse(encoded);
  } catch {
    return undefined;
  }
}

/** Validate the request's wire form against the Rust-generated branch for its command, and return that form. */
export function requireRequestShape(received: unknown): ApiRequest {
  const invalid = (): AppError => ({ code: "invalidInput", message: "The desktop request has an invalid shape.", retryable: false });
  const request = wireForm(received);
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
  return request as ApiRequest;
}
