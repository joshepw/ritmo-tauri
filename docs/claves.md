# Cómo obtener las claves

Ritmo no incluye claves. Las pegas en **Preferencias → Claves** y se guardan en el llavero de este Mac. No las subas al repositorio, no las pegues en un chat y no las compartas: quien las tenga puede usar tu cuenta en ese servicio.

Si pierdes una clave, créala de nuevo y pulsa **Quitar** en Ritmo antes de guardar la nueva.

## Gemini

Sirve para el resumen del día y las sugerencias. Ritmo la envía solo a la API de Gemini.

1. Entra en [Google AI Studio](https://aistudio.google.com/apikey) con la cuenta de Google que quieras usar.
2. Pulsa **Create API key**.
3. Elige un proyecto de Google Cloud o crea uno nuevo. El uso de la clave cuenta en la cuota de ese proyecto.
4. Copia la clave. Suele empezar por `AIza`.
5. En Ritmo, pégala en **Claves → Gemini** y pulsa **Guardar**.
6. En **Cuenta**, activa **Gemini**.

Puedes volver a ver la clave en AI Studio. Si dejas de usarla, bórrala allí y quítala también en Ritmo.

## Factorial

Sirve para cumpleaños, ausencias y el fichaje del pomodoro. Ritmo busca tu ficha con el mismo correo de la cuenta de Google.

La clave es de la empresa, no de una persona. Quien la crea tiene que ser administrador. Factorial no deja limitar lo que ve: la clave puede leer los datos de la compañía, aunque Ritmo solo pide empleados, ausencias y fichaje.

1. Entra en Factorial como administrador.
2. Abre **Configuración**.
3. Baja hasta **Configuración avanzada** y entra en **Claves de API**. En algunas cuentas aparece como **API**.
4. Crea una clave. Ponle un nombre reconocible, por ejemplo `Ritmo`.
5. Elige una caducidad larga, o una clave que no caduque. Las nuevas caducan a las 24 horas si dejas el valor por defecto, y al día siguiente Ritmo dejaría de entrar.
6. Copia la clave en cuanto aparezca. Factorial no la vuelve a mostrar.
7. En Ritmo, pégala en **Claves → Factorial** y pulsa **Guardar**.
8. En **Cuenta**, activa **Factorial**.

Tiene que ser la clave del entorno de producción (`app.factorialhr.com`). Una clave de la demo no sirve. Si el correo de Google no coincide con el de tu ficha, Ritmo muestra la empresa pero no puede fichar a tu nombre.

Guía de Factorial: [How to create API keys](https://help.factorialhr.com/en_US/managing-my-company-account/how-to-create-api-keys).

## ClickUp

Sirve para las tareas que tienes asignadas y para anotar el tiempo de un pomodoro. Ritmo usa el **primer** espacio de trabajo de la cuenta. El token puede hacer lo mismo que ese usuario.

1. Entra en ClickUp.
2. Pulsa tu avatar, arriba a la derecha, y abre **Settings**.
3. En la barra lateral, entra en **Apps**. También puedes ir directo a [app.clickup.com/settings/apps](https://app.clickup.com/settings/apps).
4. En **API Token**, pulsa **Generate**. Si ya había uno, **Regenerate** invalida el anterior.
5. Pulsa **Copy**. El token empieza por `pk_`. No le añadas `Bearer`.
6. En Ritmo, pégalo en **Claves → ClickUp** y pulsa **Guardar**.
7. En **Cuenta**, activa **ClickUp**.

El token no caduca hasta que lo regeneras. Si sospechas que alguien lo vio, regenéralo en ClickUp y guarda el nuevo en Ritmo.

Documentación de ClickUp: [Authentication](https://developer.clickup.com/docs/authentication).
