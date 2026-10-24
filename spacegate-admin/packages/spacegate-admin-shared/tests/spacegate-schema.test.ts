import { describe, expect, it } from 'vitest'
import { initSchemaDefaults, schemaKind, schemaProperties } from '../src/lib/spacegate/schema.js'

const root = {
  $defs: {
    Limits: {
      type: 'object',
      required: ['burst'],
      properties: {
        burst: { type: 'integer', default: 20 },
        cost: { type: 'number', default: 1 },
      },
    },
  },
  type: 'object',
  required: ['mode'],
  properties: {
    mode: { type: 'string', enum: ['queue', 'wait'], default: 'queue' },
    enabled: { type: 'boolean', default: true },
    limits: { $ref: '#/$defs/Limits' },
    tags: { type: 'array', items: { type: 'string' }, default: [] },
    variants: {
      oneOf: [
        { type: 'string', enum: ['a', 'b'] },
        { type: 'number', enum: [1, 2] },
      ],
    },
  },
}

describe('schema form helpers', () => {
  it('resolves local refs and flattens scalar oneOf enums', () => {
    expect(schemaKind({ $ref: '#/$defs/Limits' }, root)).toBe('object')
    expect(schemaProperties({ $ref: '#/$defs/Limits' }, root).map(([key]) => key)).toEqual(['burst', 'cost'])
    expect(schemaKind({ oneOf: [{ type: 'string', enum: ['a'] }, { type: 'string', enum: ['b'] }] }, root)).toBe('enum')
  })

  it('fills nested defaults without overwriting explicit values or unknown fields', () => {
    const existing = {
      mode: 'wait',
      enabled: false,
      limits: { cost: 0, custom: 'keep' },
      extra: { nested: [1, 2] },
      tags: ['x'],
    }
    const result = initSchemaDefaults(root, existing, root) as Record<string, unknown>
    expect(result.mode).toBe('wait')
    expect(result.enabled).toBe(false)
    expect(result.extra).toEqual({ nested: [1, 2] })
    expect(result.tags).toEqual(['x'])
    const limits = result.limits as Record<string, unknown>
    expect(limits.burst).toBe(20)
    expect(limits.cost).toBe(0)
    expect(limits.custom).toBe('keep')
  })

  it('keeps unknown array item fields while applying item defaults', () => {
    const schema = {
      type: 'array',
      items: {
        type: 'object',
        properties: {
          name: { type: 'string', default: 'item' },
          weight: { type: 'integer', default: 1 },
        },
      },
    }
    const result = initSchemaDefaults(schema, [{ weight: 0, custom: true }], schema)
    expect(result).toEqual([{ name: 'item', weight: 0, custom: true }])
  })
})
