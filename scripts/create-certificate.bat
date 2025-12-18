@echo off
:: CosmoRisk Self-Signing Script for Windows
:: This creates a self-signed certificate and signs the executable
:: Note: Self-signed certs still trigger SmartScreen, but add developer info
::
:: Run as Administrator!

echo ================================================
echo   CosmoRisk Code Signing Setup
echo   Developer: Mehmet Gumus
echo ================================================
echo.

:: Check for admin privileges
net session >nul 2>&1
if %errorlevel% neq 0 (
    echo ERROR: Please run as Administrator!
    echo Right-click and select "Run as administrator"
    pause
    exit /b 1
)

:: Create certificate
echo [1/3] Creating self-signed code signing certificate...
powershell -Command "New-SelfSignedCertificate -Type CodeSigningCert -Subject 'CN=Mehmet Gumus, O=CosmoRisk, L=Istanbul, C=TR' -KeyUsage DigitalSignature -FriendlyName 'CosmoRisk Code Signing' -CertStoreLocation 'Cert:\CurrentUser\My' -TextExtension @('2.5.29.37={text}1.3.6.1.5.5.7.3.3') -NotAfter (Get-Date).AddYears(3)"

if %errorlevel% neq 0 (
    echo ERROR: Failed to create certificate
    pause
    exit /b 1
)

echo.
echo [2/3] Certificate created successfully!
echo.

:: Get thumbprint
echo [3/3] Getting certificate thumbprint...
for /f "tokens=*" %%a in ('powershell -Command "(Get-ChildItem -Path Cert:\CurrentUser\My -CodeSigningCert | Where-Object {$_.Subject -like '*Mehmet Gumus*'} | Select-Object -First 1).Thumbprint"') do set THUMBPRINT=%%a

echo.
echo ================================================
echo   SUCCESS!
echo ================================================
echo.
echo Certificate Thumbprint: %THUMBPRINT%
echo.
echo Add this to your tauri.conf.json:
echo.
echo   "windows": {
echo     "certificateThumbprint": "%THUMBPRINT%",
echo     "timestampUrl": "http://timestamp.digicert.com"
echo   }
echo.
echo Then run: npm run tauri build
echo.
echo The exe will now show "Mehmet Gumus" as publisher!
echo.
pause
