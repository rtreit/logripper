using LogRipper.DebugHost.Components;
using LogRipper.DebugHost.Models;
using LogRipper.DebugHost.Services;

var builder = WebApplication.CreateBuilder(args);

// Add services to the container.
builder.Services.AddRazorComponents()
    .AddInteractiveServerComponents();
builder.Services.Configure<DebugWorkbenchOptions>(builder.Configuration.GetSection(DebugWorkbenchOptions.SectionName));
builder.Services.AddSingleton<RepositoryPaths>();
builder.Services.AddSingleton<ToolchainLocator>();
builder.Services.AddSingleton<ProtoJsonService>();
builder.Services.AddSingleton<SampleProtoFactory>();
builder.Services.AddScoped<DebugWorkbenchState>();
builder.Services.AddScoped<GrpcClientFactory>();
builder.Services.AddScoped<LookupWorkbenchService>();
builder.Services.AddScoped<DebugCommandService>();

var app = builder.Build();

// Configure the HTTP request pipeline.
if (!app.Environment.IsDevelopment())
{
    app.UseExceptionHandler("/Error", createScopeForErrors: true);
}


app.UseAntiforgery();

app.MapStaticAssets();
app.MapRazorComponents<App>()
    .AddInteractiveServerRenderMode();

app.Run();
