"""Current MCP schema validation, image artifacts and observation provenance."""
import base64
import json
import math
import struct
import uuid
import zlib

from common import Failure, NotImplementedCase, require


def validate(value, schema, path="arguments"):
    """Fail closed for unsupported schema keywords; not a general JSON Schema engine."""
    supported = {"type", "properties", "required", "additionalProperties", "oneOf", "anyOf",
                 "const", "enum", "items", "minItems", "maxItems", "minimum", "maximum",
                 "minLength", "maxLength", "description", "default", "title", "$schema"}
    require(isinstance(schema, dict), f"{path}: invalid schema")
    require(not (set(schema) - supported), f"{path}: unsupported schema keywords {set(schema) - supported}")
    for union in ("oneOf", "anyOf"):
        if union in schema:
            matches = 0
            for branch in schema[union]:
                try:
                    validate(value, branch, path)
                    matches += 1
                except Failure:
                    pass
            require(matches == 1 if union == "oneOf" else matches > 0, f"{path}: {union} mismatch")
    if "const" in schema:
        require(value == schema["const"], f"{path}: const mismatch")
    if "enum" in schema:
        require(value in schema["enum"], f"{path}: enum mismatch")
    kinds = {"object": lambda v: isinstance(v, dict), "array": lambda v: isinstance(v, list),
             "string": lambda v: isinstance(v, str), "integer": lambda v: type(v) is int,
             "number": lambda v: type(v) in (int, float) and math.isfinite(v),
             "boolean": lambda v: type(v) is bool, "null": lambda v: v is None}
    if "type" in schema:
        require(schema["type"] in kinds and kinds[schema["type"]](value), f"{path}: type mismatch")
    if isinstance(value, dict):
        require(set(schema.get("required", [])) <= set(value), f"{path}: required field absent")
        props = schema.get("properties", {})
        if schema.get("additionalProperties") is False:
            require(set(value) <= set(props), f"{path}: unknown fields")
        for key, child in value.items():
            if key in props:
                validate(child, props[key], path + "." + key)
    if isinstance(value, list):
        require(schema.get("minItems", 0) <= len(value) <= schema.get("maxItems", float("inf")),
                f"{path}: array size")
        if "items" in schema:
            for child in value:
                validate(child, schema["items"], path + "[]")
    if isinstance(value, str):
        require(schema.get("minLength", 0) <= len(value) <= schema.get("maxLength", float("inf")),
                f"{path}: string length")
    if type(value) in (int, float):
        require(schema.get("minimum", -float("inf")) <= value <= schema.get("maximum", float("inf")),
                f"{path}: number bounds")


def image_header(blob):
    require(len(blob) >= 33 and blob[:8] == b"\x89PNG\r\n\x1a\n", "not PNG")
    require(blob[8:16] == b"\0\0\0\rIHDR", "invalid PNG IHDR")
    require(zlib.crc32(blob[12:29]) & 0xffffffff == struct.unpack(">I", blob[29:33])[0], "IHDR CRC mismatch")
    width, height = struct.unpack(">II", blob[16:24])
    require(0 < width <= 16384 and 0 < height <= 16384, "invalid image dimensions")
    return width, height


class Computer:
    def __init__(self, rpc, evidence):
        self.rpc, self.evidence = rpc, evidence
        self.tools = {}
        self.sid = None
        self.obs = None
        self.last_sequence = None
        self.seen_observations = set()
        self.bounds = None
        self.description = None

    def initialize(self):
        result = self.rpc.call("initialize", {"protocolVersion": "2024-11-05", "capabilities": {},
                                             "clientInfo": {"name": "linux-parity-whitebox", "version": "1"}})
        require(isinstance(result, dict) and result.get("serverInfo"), "initialize missing serverInfo")
        require(result.get("protocolVersion") == "2024-11-05", "unexpected negotiated protocol")
        self.evidence.json("initialize.json", result)
        self.rpc.send("notifications/initialized", notify=True)
        listing = self.rpc.call("tools/list")
        self.evidence.json("tools.json", listing)
        require(not listing.get("nextCursor"), "paginated tools not supported by this harness")
        for tool in listing["tools"]:
            require(tool["name"] not in self.tools, "duplicate tool name")
            self.tools[tool["name"]] = tool["inputSchema"]
        needed = {"computer_describe", "computer_open", "computer_observe", "computer_step", "computer_close"}
        if not needed <= self.tools.keys():
            raise NotImplementedCase("required computer tools absent")
        _, self.description, _ = self.tool("computer_describe", {})
        self.evidence.json("describe.json", self.description)
        require(self.description.get("platform") == "linux", "not the Linux backend")
        require(self.description.get("available") is True, "Linux backend unavailable")

    def begin(self, name, args):
        if name not in self.tools:
            raise NotImplementedCase(f"tool absent: {name}")
        validate(args, self.tools[name])
        return self.rpc.send("tools/call", {"name": name, "arguments": args})

    def finish(self, rid, allow_error=False):
        result = self.rpc.receive(rid)
        require(isinstance(result, dict) and type(result.get("isError")) is bool,
                "tool result missing explicit isError")
        metadata, images, safe_content = [], [], []
        for content in result.get("content", []):
            if content.get("type") == "image":
                require(content.get("mimeType") == "image/png", "unexpected image MIME")
                blob = base64.b64decode(content["data"], validate=True)
                artifact = self.evidence.blob(blob)
                artifact["dimensions"] = list(image_header(blob))
                images.append(artifact)
                safe_content.append({"type": "image_file", **artifact})
            elif content.get("type") == "text":
                require(len(content.get("text", "")) < 1024 * 1024, "oversize metadata")
                data = json.loads(content["text"])
                require(isinstance(data, dict), "tool metadata is not object")
                metadata.append(data)
                safe_content.append({"type": "metadata", "value": data})
            else:
                raise Failure("unsupported tool content type")
        self.evidence.event("rpc_result", id=rid, isError=result["isError"], content=safe_content)
        require(len(metadata) == 1 and len(images) <= 1, "ambiguous tool content")
        if not allow_error:
            require(not result["isError"], "tool error; inspect recorded metadata")
        return result["isError"], metadata[0], images

    def tool(self, name, args, allow_error=False):
        return self.finish(self.begin(name, args), allow_error)

    def open(self, selection=None, max_width=1280, max_height=720):
        self.bounds = (max_width, max_height)
        self.last_sequence = None
        _, meta, _ = self.tool("computer_open", {"display": selection or {"kind": "primary"},
                                                "max_width": max_width, "max_height": max_height})
        self.sid = meta.get("session_id")
        require(isinstance(self.sid, str) and self.sid, "missing session id")
        self.observe()

    def accept_observation(self, meta, images):
        require(len(images) == 1, "fresh observation missing image")
        require(images[0]["dimensions"] == [meta.get("width_px"), meta.get("height_px")],
                "image header/metadata mismatch")
        require(meta.get("session_id") == self.sid, "observation session provenance mismatch")
        require(meta.get("image") == {"mime_type": "image/png", "width_px": meta.get("width_px"),
                                      "height_px": meta.get("height_px")}, "nested image metadata mismatch")
        for key in ("observation_id", "surface_id", "geometry_version", "topology_generation"):
            require(isinstance(meta.get(key), str) and meta[key], f"missing observation {key}")
        require(meta["observation_id"] not in self.seen_observations, "reused fresh observation id")
        self.seen_observations.add(meta["observation_id"])
        seq = meta.get("input_sequence")
        require(type(seq) is int and seq >= 0, "invalid input sequence")
        require(self.last_sequence is None or seq >= self.last_sequence, "regressing input sequence")
        self.last_sequence = seq
        require(meta.get("native_unit") == "physical_pixels", "Linux coordinates not root physical pixels")
        require(meta.get("mapping_regions"), "observation lacks coordinate mapping")
        require(meta.get("selected_display_ids") == [r["display_id"] for r in meta["mapping_regions"]],
                "selected display ids/mapping disagree")
        require(meta["width_px"] <= self.bounds[0] and meta["height_px"] <= self.bounds[1],
                "resize exceeds requested bound")
        for region in meta["mapping_regions"]:
            native, image = region["native_bounds"], region["image_rect"]
            require(native["width"] > 0 and native["height"] > 0, "empty native region")
            require(image["width"] > 0 and image["height"] > 0 and image["x"] >= 0 and image["y"] >= 0,
                    "invalid image region")
            require(image["x"] + image["width"] <= meta["width_px"] and
                    image["y"] + image["height"] <= meta["height_px"], "region outside image")
        self.obs = meta

    def observe(self):
        _, meta, images = self.tool("computer_observe", {"session_id": self.sid})
        self.accept_observation(meta, images)
        return meta

    def point(self, root_x, root_y):
        matches = []
        for region in self.obs["mapping_regions"]:
            n, i = region["native_bounds"], region["image_rect"]
            if n["x"] <= root_x < n["x"] + n["width"] and n["y"] <= root_y < n["y"] + n["height"]:
                x = i["x"] + (root_x - n["x"]) * i["width"] / n["width"]
                y = i["y"] + (root_y - n["y"]) * i["height"] / n["height"]
                matches.append([math.floor(x), math.floor(y)])
        require(len(matches) == 1, "fixture target has absent/ambiguous mapping")
        return matches[0]

    def step_begin(self, action):
        require(self.obs is not None, "step without observation")
        request_id = "parity-" + uuid.uuid4().hex
        args = {"session_id": self.sid, "request_id": request_id,
                "based_on": self.obs["observation_id"], "action": action}
        return self.begin("computer_step", args), request_id

    def step_finish(self, token, cancellation=False):
        rid, request_id = token
        error, meta, images = self.finish(rid, allow_error=True)
        require(meta.get("request_id") == request_id, "step request_id mismatch")
        require(meta.get("input_outcome") in ("not_started", "dispatched", "partial", "unknown"),
                "invalid input outcome")
        require(meta.get("cleanup_outcome") in ("not_needed", "released", "failed", "unknown"),
                "invalid cleanup outcome")
        if cancellation:
            # All fields stay in evidence, including honest partial/unknown. Unknown
            # is not sufficient to establish successful cancellation.
            require(meta.get("cancelled") is True and meta["input_outcome"] == "partial",
                    "observed held input but no confirmed partial cancellation")
            require(meta["cleanup_outcome"] == "released", "cancel cleanup not confirmed released")
        else:
            require(not error and meta["input_outcome"] == "dispatched", "input not fully dispatched")
            require(meta["cleanup_outcome"] in ("not_needed", "released"), "cleanup failed/unknown")
            require(meta.get("observation_outcome") == "available", "step observation unavailable")
            self.accept_observation(meta["observation"], images)
        return meta

    def step(self, action):
        return self.step_finish(self.step_begin(action))

    def close(self):
        if self.sid is not None:
            _, meta, _ = self.tool("computer_close", {"session_id": self.sid})
            require(meta.get("state") == "closed", "close did not close")
            require(meta.get("cleanup_outcome") in ("released", "not_needed"), "close cleanup uncertain")
            self.sid = None
            self.obs = None
