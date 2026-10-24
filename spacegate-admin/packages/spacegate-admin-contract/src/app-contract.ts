import { apiContract } from '@h-ai/api-contract'
import { AppEchoInputSchema, AppEchoOutputDataSchema, AppInfoOutputDataSchema } from './app-schemas.js'

/** app 领域 API contract：路径、方法、输入输出 Schema 的单一来源。 */
export const appContract = {
  info: apiContract.route({
    method: 'POST',
    path: '/app/info',
    operationId: 'app.info',
    tags: ['app'],
    summary: 'Get application runtime information',
  }).output(apiContract.haiResultSchema(AppInfoOutputDataSchema)),
  echo: apiContract.route({
    method: 'POST',
    path: '/app/echo',
    operationId: 'app.echo',
    tags: ['app'],
    summary: 'Echo a message through the generated service route',
  }).input(AppEchoInputSchema).output(apiContract.haiResultSchema(AppEchoOutputDataSchema)),
}
