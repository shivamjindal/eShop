global using Microsoft.VisualStudio.TestTools.UnitTesting;
global using eShop.Catalog.API.Infrastructure.Exceptions;
global using eShop.Catalog.API.Model;

[assembly: Parallelize(Workers = 0, Scope = ExecutionScope.MethodLevel)]
