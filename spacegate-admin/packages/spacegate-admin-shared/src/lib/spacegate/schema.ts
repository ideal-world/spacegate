import { asRecord, cloneValue, readBoolean, readNumber, readString } from './model.js'

/** 运行时插件 schema 仍是 JSON Schema 子集；保留宽松对象以避免拒绝上游新增关键字。 */
export type SchemaRecord = Record<string, unknown>

function asSchema(value: unknown): SchemaRecord | null {
  return asRecord(value)
}

/**
 * 解析本地 `$ref`、`allOf` 与 `anyOf`，并合并相邻 title/description。
 * 只支持 `#/...` 本地引用；远程引用在离线表单中保持原样并回退 JSON 编辑。
 * @example resolveSchema({ $ref: '#/$defs/Config' }, root)
 */
export function resolveSchema(schema: unknown, root: unknown = schema, depth = 0): SchemaRecord {
  const record = asSchema(schema)
  if (!record || depth > 16)
    return record ?? {}

  const reference = readString(record, '$ref')
  if (reference.startsWith('#/')) {
    const target = reference.slice(2).split('/').reduce<unknown>((current, segment) => {
      const source = asSchema(current)
      return source?.[segment.replace(/~1/g, '/').replace(/~0/g, '~')]
    }, root)
    const resolved = resolveSchema(target, root, depth + 1)
    const { $ref: _reference, ...overrides } = record
    return mergeSchemas(resolved, overrides)
  }

  const allOf = Array.isArray(record.allOf) ? record.allOf : []
  if (allOf.length > 0) {
    const { allOf: _allOf, ...rest } = record
    return allOf.reduce<SchemaRecord>((current, branch) => mergeSchemas(current, resolveSchema(branch, root, depth + 1)), rest)
  }

  const anyOf = Array.isArray(record.anyOf) ? record.anyOf : []
  if (anyOf.length > 0) {
    const { anyOf: _anyOf, ...rest } = record
    const branch = anyOf.map(item => resolveSchema(item, root, depth + 1)).find(item => readString(item, 'type') !== 'null')
    return branch ? mergeSchemas(branch, rest) : rest
  }

  return record
}

/** 合并 JSON schema 的常用结构，保留未知关键字。 */
function mergeSchemas(base: SchemaRecord, extra: SchemaRecord): SchemaRecord {
  const merged: SchemaRecord = { ...base, ...extra }
  const baseProperties = asSchema(base.properties)
  const extraProperties = asSchema(extra.properties)
  if (baseProperties || extraProperties)
    merged.properties = { ...baseProperties, ...extraProperties }
  const baseRequired = Array.isArray(base.required) ? base.required.filter(item => typeof item === 'string') : []
  const extraRequired = Array.isArray(extra.required) ? extra.required.filter(item => typeof item === 'string') : []
  if (baseRequired.length > 0 || extraRequired.length > 0)
    merged.required = [...new Set([...baseRequired, ...extraRequired])]
  return merged
}

/** SchemaForm 渲染种类。 */
export type SchemaKind = 'object' | 'array' | 'string' | 'number' | 'integer' | 'boolean' | 'enum' | 'oneOf' | 'json'

/** 枚举候选值。 */
export interface SchemaEnumOption {
  value: unknown
  label: string
}

/** 读取非 null 的 JSON schema 类型。 */
function schemaType(schema: SchemaRecord): string {
  const type = schema.type
  if (Array.isArray(type)) {
    const nonNull = type.find(item => typeof item === 'string' && item !== 'null')
    return typeof nonNull === 'string' ? nonNull : ''
  }
  return typeof type === 'string' ? type : ''
}

/** 判断 schema 的渲染种类。 */
export function schemaKind(schema: unknown, root: unknown = schema): SchemaKind {
  const record = resolveSchema(schema, root)
  if (Array.isArray(record.enum))
    return 'enum'
  if (Array.isArray(record.oneOf)) {
    return enumOptions(record).length > 0 ? 'enum' : 'oneOf'
  }
  const type = schemaType(record)
  if (type === 'object' || record.properties)
    return 'object'
  if (type === 'array' || record.items)
    return 'array'
  if (type === 'integer')
    return 'integer'
  if (type === 'number')
    return 'number'
  if (type === 'boolean')
    return 'boolean'
  if (type === 'string')
    return 'string'
  return 'json'
}

/** 直接 enum 与同类型 oneOf enum 的扁平化候选。 */
export function enumOptions(schema: unknown, root: unknown = schema): SchemaEnumOption[] {
  const record = resolveSchema(schema, root)
  const values: unknown[] = Array.isArray(record.enum) ? record.enum : []
  if (values.length === 0 && Array.isArray(record.oneOf)) {
    const branches = record.oneOf.map(item => resolveSchema(item, root))
    const scalarBranches = branches.filter(branch => Array.isArray(branch.enum))
    if (scalarBranches.length === branches.length) {
      for (const branch of scalarBranches)
        values.push(...(branch.enum as unknown[]))
    }
  }
  return [...new Map(values.map(value => [JSON.stringify(value), value])).values()].map(value => ({
    value,
    label: enumLabel(value),
  }))
}

function enumLabel(value: unknown): string {
  if (typeof value === 'string')
    return value
  if (value === null)
    return 'null'
  return JSON.stringify(value)
}

/** 枚举值在 Select 中的稳定字符串编码。 */
export function enumValueToken(value: unknown): string {
  return JSON.stringify(value)
}

/** 把 Select 字符串还原为枚举原始值。 */
export function enumValueFromToken(token: string): unknown {
  try {
    return JSON.parse(token)
  }
  catch {
    return token
  }
}

/** 对象字段名列表。 */
export function schemaProperties(schema: unknown, root: unknown = schema): Array<[string, SchemaRecord]> {
  const record = resolveSchema(schema, root)
  const properties = asSchema(record.properties)
  return properties ? Object.entries(properties).map(([key, value]) => [key, asSchema(value) ?? {}]) : []
}

/** 必填字段集合，嵌套对象由递归节点各自判断。 */
export function schemaRequired(schema: unknown, root: unknown = schema): Set<string> {
  const record = resolveSchema(schema, root)
  return new Set(Array.isArray(record.required) ? record.required.filter(item => typeof item === 'string') : [])
}

/** 字段显示标题；优先 title，其次字段名。 */
export function schemaTitle(schema: unknown, fallback: string, root: unknown = schema): string {
  return readString(resolveSchema(schema, root), 'title') || fallback
}

/** 字段说明。 */
export function schemaDescription(schema: unknown, root: unknown = schema): string {
  return readString(resolveSchema(schema, root), 'description')
}

/** 数组项 schema；缺失时按任意 JSON 对象处理。 */
export function schemaItems(schema: unknown, root: unknown = schema): SchemaRecord {
  return asSchema(resolveSchema(schema, root).items) ?? {}
}

/** oneOf 分支。 */
export function schemaBranches(schema: unknown, root: unknown = schema): SchemaRecord[] {
  const record = resolveSchema(schema, root)
  return Array.isArray(record.oneOf) ? record.oneOf.map(item => resolveSchema(item, root)) : []
}

/**
 * 按 schema 填充缺失默认值，同时保留已有值和未知字段。
 * 与旧实现不同：不覆盖显式传入的 null/false/0，并且递归处理嵌套对象与数组项。
 * @example initSchemaDefaults({ type: 'object', properties: { mode: { default: 'queue' } } }, {})
 */
export function initSchemaDefaults(schema: unknown, value: unknown, root: unknown = schema, depth = 0): unknown {
  if (depth > 16)
    return cloneValue(value)
  const record = resolveSchema(schema, root)
  if (Array.isArray(record.oneOf) && !Array.isArray(record.enum)) {
    const branches = schemaBranches(record, root)
    const branch = branches.find(item => valueMatchesSchema(item, value)) ?? branches[0]
    return branch ? initSchemaDefaults(branch, value, root, depth + 1) : cloneValue(value)
  }
  const kind = schemaKind(record, root)
  if (kind === 'object') {
    const source = asRecord(value)
    const result: Record<string, unknown> = source ? cloneValue(source) : {}
    for (const [key, property] of schemaProperties(record, root)) {
      if (result[key] !== undefined) {
        result[key] = initSchemaDefaults(property, result[key], root, depth + 1)
        continue
      }
      const propertyRecord = resolveSchema(property, root)
      if (propertyRecord.default !== undefined)
        result[key] = cloneValue(propertyRecord.default)
    }
    return result
  }
  if (kind === 'array') {
    const items = schemaItems(record, root)
    const source = Array.isArray(value) ? value : Array.isArray(record.default) ? record.default : value
    if (!Array.isArray(source))
      return cloneValue(value)
    return source.map(item => initSchemaDefaults(items, item, root, depth + 1))
  }
  if (value !== undefined)
    return cloneValue(value)
  return record.default === undefined ? undefined : cloneValue(record.default)
}

/**
 * 根据 schema 生成示例值，优先使用 default/example/examples，其次按类型生成占位值。
 * 与旧管理端 PluginForm 的 `exampleValue` 行为一致，用于“填入示例 / 复制示例”按钮。
 * @example schemaExample({ type: 'object', properties: { enabled: { type: 'boolean' } } })
 */
export function schemaExample(schema: unknown, root: unknown = schema, depth = 0): unknown {
  if (depth > 4)
    return {}
  const record = resolveSchema(schema, root)
  if (record.default !== undefined)
    return cloneValue(record.default)
  if (record.example !== undefined)
    return cloneValue(record.example)
  if (Array.isArray(record.examples) && record.examples.length > 0)
    return cloneValue(record.examples[0])
  const kind = schemaKind(record, root)
  if (kind === 'object') {
    const result: Record<string, unknown> = {}
    for (const [key, property] of schemaProperties(record, root))
      result[key] = schemaExample(property, root, depth + 1)
    return result
  }
  if (kind === 'array')
    return []
  if (kind === 'boolean')
    return record.default === undefined ? false : cloneValue(record.default)
  if (kind === 'number' || kind === 'integer')
    return record.default === undefined ? 0 : cloneValue(record.default)
  if (kind === 'enum') {
    const options = enumOptions(record, root)
    return options[0]?.value ?? ''
  }
  if (kind === 'json')
    return {}
  return ''
}

function valueMatchesSchema(schema: SchemaRecord, value: unknown): boolean {
  const type = schemaType(schema)
  if (!type)
    return true
  if (type === 'null')
    return value === null
  if (type === 'array')
    return Array.isArray(value)
  if (type === 'object')
    return asRecord(value) !== null
  if (type === 'integer')
    return typeof value === 'number' && Number.isInteger(value)
  if (type === 'number')
    return typeof value === 'number'
  if (type === 'string')
    return typeof value === 'string'
  if (type === 'boolean')
    return typeof value === 'boolean'
  return false
}

/** 数组项默认值：对象按 schema 默认值初始化，标量取 default 或空值。 */
export function newSchemaArrayItem(schema: unknown, root: unknown = schema): unknown {
  const record = resolveSchema(schema, root)
  if (record.default !== undefined)
    return cloneValue(record.default)
  const kind = schemaKind(record, root)
  if (kind === 'object')
    return initSchemaDefaults(record, {}, root)
  if (kind === 'array')
    return []
  if (kind === 'boolean')
    return false
  if (kind === 'integer')
    return 0
  if (kind === 'number')
    return 0
  if (kind === 'string')
    return ''
  return null
}

/** 表单文本输入的数字/布尔转换；空字符串表示未设置。 */
export function schemaInputValue(value: unknown): string {
  return typeof value === 'string' || typeof value === 'number' ? String(value) : ''
}

/** 数字输入解析；保留负数和 0，空值回到 undefined。 */
export function parseNumberInput(value: string, integer = false): number | undefined {
  if (!value.trim())
    return undefined
  const parsed = Number(value)
  if (!Number.isFinite(parsed) || (integer && !Number.isInteger(parsed)))
    return undefined
  return parsed
}

/** 读取 schema 默认的布尔/数字字段，避免 UI 反复判空。 */
export function schemaBooleanDefault(schema: unknown, root: unknown = schema): boolean {
  return readBoolean(resolveSchema(schema, root), 'default') ?? false
}

/** 读取 schema 默认数字。 */
export function schemaNumberDefault(schema: unknown, root: unknown = schema): number | undefined {
  return readNumber(resolveSchema(schema, root), 'default')
}
