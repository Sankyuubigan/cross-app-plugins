## Default Permission

Default permissions for the audio-gen plugin (local audio.cpp generation).

#### This default permission set includes the following:

- `allow-audio-gen-list-models`
- `allow-audio-gen-generate`
- `allow-audio-gen-status`
- `allow-audio-gen-cancel`
- `allow-audio-gen-open-output-dir`

## Permission Table

<table>
<tr>
<th>Identifier</th>
<th>Description</th>
</tr>


<tr>
<td>

`plugin-audio-gen-engine:allow-audio-gen-cancel`

</td>
<td>

Enables the audio_gen_cancel command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`plugin-audio-gen-engine:deny-audio-gen-cancel`

</td>
<td>

Denies the audio_gen_cancel command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`plugin-audio-gen-engine:allow-audio-gen-generate`

</td>
<td>

Enables the audio_gen_generate command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`plugin-audio-gen-engine:deny-audio-gen-generate`

</td>
<td>

Denies the audio_gen_generate command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`plugin-audio-gen-engine:allow-audio-gen-list-models`

</td>
<td>

Enables the audio_gen_list_models command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`plugin-audio-gen-engine:deny-audio-gen-list-models`

</td>
<td>

Denies the audio_gen_list_models command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`plugin-audio-gen-engine:allow-audio-gen-open-output-dir`

</td>
<td>

Enables the audio_gen_open_output_dir command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`plugin-audio-gen-engine:deny-audio-gen-open-output-dir`

</td>
<td>

Denies the audio_gen_open_output_dir command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`plugin-audio-gen-engine:allow-audio-gen-status`

</td>
<td>

Enables the audio_gen_status command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`plugin-audio-gen-engine:deny-audio-gen-status`

</td>
<td>

Denies the audio_gen_status command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`plugin-audio-gen-engine:allow-audio-gen-list-models`

</td>
<td>

Allow listing known audio models (SA3/MOSS/ACE-Step) with install status.

</td>
</tr>

<tr>
<td>

`plugin-audio-gen-engine:allow-audio-gen-generate`

</td>
<td>

Allow starting a local SFX/music generation job (audio.cpp engine).

</td>
</tr>

<tr>
<td>

`plugin-audio-gen-engine:allow-audio-gen-status`

</td>
<td>

Allow polling job status/progress.

</td>
</tr>

<tr>
<td>

`plugin-audio-gen-engine:allow-audio-gen-cancel`

</td>
<td>

Allow cancelling the running/queued generation job.

</td>
</tr>

<tr>
<td>

`plugin-audio-gen-engine:allow-audio-gen-open-output-dir`

</td>
<td>

Allow opening the output folder in Explorer.

</td>
</tr>
</table>
