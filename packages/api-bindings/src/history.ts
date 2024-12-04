// eslint-disable-next-line camelcase
import { HistoryQueryRequest_Param } from "@aws/amazon-q-developer-cli-proto/fig";
import { sendHistoryQueryRequest } from "./requests.js";

export type Param = string | number | Uint8Array | null;

// eslint-disable-next-line camelcase
function mapParam(param: Param): HistoryQueryRequest_Param {
  if (param === null) {
    return {
      type: {
        $case: "null",
        null: {},
      },
    };
  }

  if (typeof param === "string") {
    return {
      type: {
        $case: "string",
        string: param,
      },
    };
  }

  if (typeof param === "number" && Number.isInteger(param)) {
    return {
      type: {
        $case: "integer",
        integer: param,
      },
    };
  }

  if (typeof param === "number") {
    return {
      type: {
        $case: "float",
        float: param,
      },
    };
  }

  if (param instanceof Uint8Array) {
    return {
      type: {
        $case: "blob",
        blob: param,
      },
    };
  }

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
