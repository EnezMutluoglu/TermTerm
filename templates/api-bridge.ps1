param([Parameter(Mandatory)][string]$BaseUrl,[Parameter(Mandatory)][string]$Token)
# Start the bridge in Settings > Integrations, then pass its local URL and token.
$headers=@{Authorization="Bearer $Token"}
Invoke-RestMethod -Uri "$BaseUrl/v1/hosts" -Headers $headers
$body=@{label='Example server';address='192.0.2.10';port=22;username='ubuntu'} | ConvertTo-Json
Invoke-RestMethod -Uri "$BaseUrl/v1/hosts" -Method Post -Headers $headers -ContentType 'application/json' -Body $body
