import {
  HistoryQueryRequest_Param,
  HistoryQueryRequest_ParamSchema,
} from "@aws/amazon-q-developer-cli-proto/fig";
import { sendHistoryQueryRequest } from "./requests.js";
import { create } from "@bufbuild/protobuf";
import { EmptySchema } from "@aws/amazon-q-developer-cli-proto/fig_common";

export type Param = string | number | Uint8Array | null;

function mapParam(param: Param): HistoryQueryRequest_Param {
  let type: HistoryQueryRequest_Param["type"] | undefined;

  if (param === null) {
    type = {
      case: "null",
      value: create(EmptySchema),
    };
  } else if (typeof param === "string") {
    type = { case: "string", value: param };
  } else if (typeof param === "number" && Number.isInteger(param)) {
    type = {
      case: "integer",
      value: BigInt(param),
    };
  } else if (typeof param === "number") {
    type = {
      case: "float",
      value: param,
    };
  } else if (param instanceof Uint8Array) {
    type = {
      case: "blob",
      value: param,
    };
  }

  if (type) return create(HistoryQueryRequest_ParamSchema, { type });
  throw new Error("Invalid param type");
}

export async function query(
  sql: string,
  params?: Param[],
): Promise<Array<Record<string, unknown>>> {
  const response = await sendHistoryQueryRequest({
    query: sql,
    params: params ? params.map(mapParam) : [],
  });
  return JSON.parse(response.jsonArray);
}
